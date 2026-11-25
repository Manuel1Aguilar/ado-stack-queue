use crate::engine::Engine;
use crate::model::*;
use crate::ports::*;
use crate::telemetry::{BatchResult, Event};
use std::collections::HashMap;

enum Standing {
    Merged,
    Ready,
    Stale,
}

pub fn split(group: &[Member]) -> (Vec<Member>, Vec<Member>) {
    let (left, right) = group.split_at(group.len().div_ceil(2));
    (left.to_vec(), right.to_vec())
}

impl Engine {
    pub(crate) fn advance_queue(&self, prs: &[Pr]) -> Result<()> {
        let queued = self.sync_queue(prs)?;
        match self.store.batch()? {
            Some(batch) => self.advance(batch, prs, &queued),
            None => self.form(prs, &queued),
        }
    }

    fn sync_queue(&self, prs: &[Pr]) -> Result<Vec<QueueEntry>> {
        let known = self.store.queued()?;
        let live: Vec<&Pr> = prs.iter().filter(|p| p.is_queued()).collect();
        for entry in known.iter().filter(|e| !live.iter().any(|p| p.id == e.pr)) {
            self.store.unqueue(entry.pr)?;
        }
        let mut entries: Vec<QueueEntry> = known
            .into_iter()
            .filter(|e| live.iter().any(|p| p.id == e.pr))
            .collect();
        let fresh: Vec<&&Pr> = live
            .iter()
            .filter(|p| !entries.iter().any(|e| e.pr == p.id))
            .collect();
        for pr in fresh {
            let entry = QueueEntry {
                pr: pr.id,
                seq: self.store.next_seq("queued")?,
                at_ms: self.clock.now_ms(),
            };
            self.store.enqueue(&entry)?;
            entries.push(entry);
        }
        entries.sort_by_key(|e| (e.seq, e.pr));
        Ok(entries)
    }

    fn form(&self, prs: &[Pr], queued: &[QueueEntry]) -> Result<()> {
        let by_id: HashMap<PrId, &Pr> = prs.iter().map(|p| (p.id, p)).collect();
        let waiting: Vec<&Pr> = queued
            .iter()
            .map(|e| by_id[&e.pr])
            .filter(|p| {
                !prs.iter()
                    .any(|q| q.state == PrState::Active && q.source == p.target)
            })
            .collect();
        let Some(first) = waiting.first() else {
            return Ok(());
        };
        let members: Vec<Member> = waiting
            .iter()
            .filter(|p| p.target == first.target)
            .take(self.config.batch_max)
            .map(|p| Member {
                pr: p.id,
                sha: p.source_sha.clone(),
            })
            .collect();
        let mut batch = Batch {
            id: self.store.next_seq("batch")?,
            target: first.target.clone(),
            groups: vec![members],
            running: None,
            builds: 0,
        };
        self.store.save_batch(&batch)?;
        self.start(&mut batch)
    }

    fn start(&self, batch: &mut Batch) -> Result<()> {
        loop {
            let Some(group) = batch.groups.first() else {
                return self.store.clear_batch();
            };
            let queue_ref = Ref(format!(
                "refs/heads/queue/{}",
                self.store.next_seq("queue_ref")?
            ));
            match self.git.build_queue_ref(&batch.target, group, &queue_ref)? {
                QueueRef::Conflict { pr } => {
                    let size = group.len();
                    batch.groups[0].retain(|m| m.pr != pr);
                    self.evict(pr, "does not merge cleanly into the queue")?;
                    self.events.emit(&Event::Batch {
                        size,
                        result: BatchResult::Conflict,
                        builds: batch.builds,
                    })?;
                    if batch.groups[0].is_empty() {
                        batch.groups.remove(0);
                    }
                }
                QueueRef::Built => {
                    let build = self.builds.queue(&queue_ref)?;
                    batch.builds += 1;
                    batch.running = Some(Running { build, queue_ref });
                    return self.store.save_batch(batch);
                }
            }
        }
    }

    fn advance(&self, mut batch: Batch, prs: &[Pr], queued: &[QueueEntry]) -> Result<()> {
        let Some(running) = batch.running.clone() else {
            return self.start(&mut batch);
        };
        match self.builds.poll(running.build)? {
            BuildOutcome::Pending => Ok(()),
            BuildOutcome::Succeeded => self.land(batch, running, prs, queued),
            BuildOutcome::Failed => {
                let group = batch.groups.remove(0);
                self.events.emit(&Event::Batch {
                    size: group.len(),
                    result: BatchResult::Red,
                    builds: batch.builds,
                })?;
                self.git.delete_ref(&running.queue_ref)?;
                batch.running = None;
                if let [culprit] = group.as_slice() {
                    self.evict(culprit.pr, "queue build failed")?;
                } else {
                    let (left, right) = split(&group);
                    batch.groups.splice(0..0, [left, right]);
                }
                self.start(&mut batch)
            }
        }
    }

    fn land(
        &self,
        mut batch: Batch,
        running: Running,
        prs: &[Pr],
        queued: &[QueueEntry],
    ) -> Result<()> {
        let by_id: HashMap<PrId, &Pr> = prs.iter().map(|p| (p.id, p)).collect();
        let standing = |m: &Member| match by_id.get(&m.pr) {
            Some(p) if p.state == PrState::Completed => Standing::Merged,
            Some(p) if p.is_queued() && p.source_sha == m.sha => Standing::Ready,
            _ => Standing::Stale,
        };
        let group = batch.groups[0].clone();
        if group.iter().any(|m| matches!(standing(m), Standing::Stale)) {
            for m in group
                .iter()
                .filter(|m| matches!(standing(m), Standing::Stale))
            {
                self.store.unqueue(m.pr)?;
            }
            self.events.emit(&Event::Batch {
                size: group.len(),
                result: BatchResult::Stale,
                builds: batch.builds,
            })?;
            batch.groups[0].retain(|m| matches!(standing(m), Standing::Ready));
            if batch.groups[0].is_empty() {
                batch.groups.remove(0);
            }
            self.git.delete_ref(&running.queue_ref)?;
            batch.running = None;
            return self.start(&mut batch);
        }
        let now = self.clock.now_ms();
        for m in group
            .iter()
            .filter(|m| matches!(standing(m), Standing::Ready))
        {
            self.ado.complete(m.pr, &m.sha)?;
            let waited = queued
                .iter()
                .find(|e| e.pr == m.pr)
                .map_or(0, |e| now.saturating_sub(e.at_ms) / 1000);
            self.events.emit(&Event::Merge {
                pr: m.pr,
                wait_s: waited,
            })?;
        }
        self.events.emit(&Event::Batch {
            size: group.len(),
            result: BatchResult::Green,
            builds: batch.builds,
        })?;
        self.git.delete_ref(&running.queue_ref)?;
        batch.groups.remove(0);
        batch.running = None;
        self.start(&mut batch)
    }

    fn evict(&self, pr: PrId, why: &str) -> Result<()> {
        self.post(pr, Check::Queue, Verdict::Failed(why.to_string()))?;
        self.ado.remove_label(pr, QUEUE_LABEL)?;
        self.store.unqueue(pr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members(n: u64) -> Vec<Member> {
        (1..=n)
            .map(|i| Member {
                pr: PrId(i),
                sha: Sha(format!("sha{i}")),
            })
            .collect()
    }

    #[test]
    fn split_halves_keep_order_and_the_larger_half_goes_first() {
        let (left, right) = split(&members(5));
        assert_eq!(left.iter().map(|m| m.pr.0).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(right.iter().map(|m| m.pr.0).collect::<Vec<_>>(), [4, 5]);
    }

    #[test]
    fn actions_parse_from_name_and_payload() {
        assert_eq!(Action::parse("Tick", None), Ok(Action::Tick));
        assert_eq!(
            Action::parse("Enqueue", Some("{\"pr\":7}")),
            Ok(Action::Enqueue { pr: PrId(7) })
        );
        assert!(Action::parse("Nope", None).unwrap_err().contains("Dequeue"));
    }
}
