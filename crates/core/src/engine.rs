use crate::model::*;
use crate::ports::*;
use crate::telemetry::Event;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub batch_max: usize,
}

pub struct Engine {
    pub ado: Box<dyn AdoApi>,
    pub git: Box<dyn Git>,
    pub builds: Box<dyn Builds>,
    pub store: Box<dyn Store>,
    pub events: Box<dyn Events>,
    pub clock: Box<dyn Clock>,
    pub config: Config,
}

impl Engine {
    pub fn apply(&self, action: Action) -> Result<()> {
        match action {
            Action::Tick => self.tick(),
            Action::Enqueue { pr } => {
                self.ado.add_label(pr, QUEUE_LABEL)?;
                if self.store.queued()?.iter().all(|e| e.pr != pr) {
                    let entry = QueueEntry {
                        pr,
                        seq: self.store.next_seq("queued")?,
                        at_ms: self.clock.now_ms(),
                    };
                    self.store.enqueue(&entry)?;
                }
                Ok(())
            }
            Action::Dequeue { pr } => {
                self.ado.remove_label(pr, QUEUE_LABEL)?;
                self.store.unqueue(pr)
            }
            Action::Pause => self.store.set_paused(true),
            Action::Resume => self.store.set_paused(false),
        }
    }

    pub fn tick(&self) -> Result<()> {
        if self.store.paused()? {
            return Ok(());
        }
        let started = self.clock.now_ms();
        let prs = self.ado.list_prs()?;
        self.reconcile_stacks(&prs)?;
        self.advance_queue(&prs)?;
        self.events.emit(&Event::Tick {
            ms: self.clock.now_ms() - started,
            prs: prs.len(),
        })
    }

    pub fn view(&self) -> Result<View> {
        let prs = self.ado.list_prs()?;
        let by_source: HashMap<&Ref, &Pr> = prs.iter().map(|p| (&p.source, p)).collect();
        let mut stacks: Vec<Vec<PrId>> = Vec::new();
        for root in prs
            .iter()
            .filter(|p| p.state == PrState::Active && !by_source.contains_key(&p.target))
        {
            let mut chain = vec![root.id];
            let mut tip = root;
            while let Some(child) = prs
                .iter()
                .find(|c| c.state == PrState::Active && c.target == tip.source)
            {
                chain.push(child.id);
                tip = child;
            }
            if chain.len() > 1 {
                stacks.push(chain);
            }
        }
        Ok(View {
            paused: self.store.paused()?,
            stacks,
            queue: self.store.queued()?,
            batches: self.store.batch()?.into_iter().collect(),
        })
    }

    pub(crate) fn post(&self, pr: PrId, check: Check, verdict: Verdict) -> Result<()> {
        if self.store.posted(pr, check)?.as_ref() == Some(&verdict) {
            return Ok(());
        }
        self.ado.set_status(pr, check, &verdict)?;
        self.store.set_posted(pr, check, &verdict)
    }
}
