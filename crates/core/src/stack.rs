use crate::engine::Engine;
use crate::model::*;
use crate::ports::*;
use crate::telemetry::Event;

impl Engine {
    pub(crate) fn reconcile_stacks(&self, prs: &[Pr]) -> Result<()> {
        for parent in prs.iter().filter(|p| p.state == PrState::Completed) {
            for child in prs
                .iter()
                .filter(|c| c.state == PrState::Active && c.target == parent.source)
            {
                self.restack(prs, child, parent)?;
            }
        }
        Ok(())
    }

    fn restack(&self, prs: &[Pr], child: &Pr, parent: &Pr) -> Result<()> {
        if self.store.stack_failure(child.id)?.as_ref() == Some(&child.source_sha) {
            return Ok(());
        }
        self.ado.retarget(child.id, &parent.target)?;
        let ok = self.ado.get_pr(child.id)?.target == parent.target;
        self.events.emit(&Event::Retarget { pr: child.id, ok })?;
        if !ok {
            let why = "retargeting did not change targetRefName; is PR retargeting enabled for the repository?";
            return self.fail_stack(child, why.to_string());
        }
        self.rebase_chain(prs, child, &parent.target, &parent.source_sha)
    }

    fn rebase_chain(&self, prs: &[Pr], child: &Pr, base: &Ref, old_base: &Sha) -> Result<()> {
        let started = self.clock.now_ms();
        let outcome = self
            .git
            .rebase_onto(&child.source, base, old_base, &child.source_sha);
        let ms = self.clock.now_ms() - started;
        match outcome {
            Ok(Rebase::Done { .. }) => {
                self.events.emit(&Event::Rebase {
                    pr: child.id,
                    conflict: false,
                    ms,
                })?;
                self.post(child.id, Check::Stack, Verdict::Succeeded)?;
                for grandchild in prs
                    .iter()
                    .filter(|g| g.state == PrState::Active && g.target == child.source)
                {
                    self.rebase_chain(prs, grandchild, &child.source, &child.source_sha)?;
                }
                Ok(())
            }
            Ok(Rebase::Conflict { message }) => {
                self.events.emit(&Event::Rebase {
                    pr: child.id,
                    conflict: true,
                    ms,
                })?;
                self.fail_stack(child, message)
            }
            Err(e) => self.fail_stack(child, e.0),
        }
    }

    fn fail_stack(&self, child: &Pr, message: String) -> Result<()> {
        self.store.set_stack_failure(child.id, &child.source_sha)?;
        self.post(child.id, Check::Stack, Verdict::Failed(message))
    }
}
