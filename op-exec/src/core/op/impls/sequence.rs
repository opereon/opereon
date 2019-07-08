use super::*;

#[derive(Debug)]
pub struct SequenceOperation {
    engine: EngineRef,
    operation: OperationRef,
    steps: Vec<OperationExec>,
    outcomes: Vec<Outcome>,
    current_step: Option<usize>,
}

impl SequenceOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, steps: Vec<OperationRef>) -> Result<SequenceOperation, RuntimeError> {
        let n = steps.len();
        let mut steps_ = Vec::with_capacity(steps.len());
        for s in steps {
            s.write().block(true);
            let step = engine.enqueue_operation(s.clone(), false)?.into_exec();
            steps_.push(step);
        }
        Ok(SequenceOperation {
            engine,
            operation,
            steps: steps_,
            outcomes: Vec::with_capacity(n),
            current_step: None,
        })
    }
}

impl Future for SequenceOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(s) = self.current_step.take() {
            if let Async::Ready(Some(p)) = self.steps[s].progress.poll()? {
                self.operation.write().update_progress( p)
//                self.operation.write().update_progress_step(self.outcomes.len(), p)
            }
            match self.steps[s].outcome.poll()? {
                Async::NotReady => {
                    self.current_step = Some(s);
                    Ok(Async::NotReady)
                },
                Async::Ready(outcome) => {
                    self.outcomes.push(outcome);
                    self.operation.read().task.notify();
                    Ok(Async::NotReady)
                }
            }
        } else {
            let curr = self.outcomes.len();
            if curr == self.steps.len() {
                let mut outcomes = Vec::new();
                std::mem::swap(&mut self.outcomes, &mut outcomes);
                Ok(Async::Ready(Outcome::Many(outcomes)))
            } else {
                self.current_step = Some(curr);
                self.engine.block_operation(&self.steps[curr].operation, false);
                self.operation.read().task.notify();
                Ok(Async::NotReady)
            }
        }
    }
}

impl OperationImpl for SequenceOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        self.operation.write().progress = Progress::from_steps(self.steps.iter().map(|o| o.operation.read().progress.clone()).collect());
        Ok(())
    }
}

unsafe impl Sync for SequenceOperation {}

unsafe impl Send for SequenceOperation {}
