use super::*;

#[derive(Debug)]
pub struct ParallelOperation {
    engine: EngineRef,
    operation: OperationRef,
    steps: Vec<OperationExec>,
    outcomes: Vec<Option<Outcome>>,
    count: usize,

    steps_sync: Vec<OperationRef>
}

impl ParallelOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, steps: Vec<OperationRef>) -> Result<ParallelOperation, RuntimeError> {
        let n = steps.len();
        let mut steps_ = Vec::with_capacity(steps.len());
        for s in &steps {
            let step = engine.enqueue_operation(s.clone(), false)?.into_exec();
            steps_.push(step);
        }
        Ok(ParallelOperation {
            engine,
            operation,
            steps: steps_,
            outcomes: vec![None; n],
            count: 0,

            steps_sync: steps
        })
    }
}

impl Future for ParallelOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        for (i, s) in self.steps.iter_mut().enumerate() {
            if self.outcomes[i].is_none() {
                if let Async::Ready(Some(p)) = s.progress.poll()? {
                    self.operation.write().update_progress( p);
//                    self.operation.write().update_progress_step(i, p);
                }
                if let Async::Ready(outcome) = s.outcome.poll()? {
                    self.outcomes[i] = Some(outcome);
                    self.count += 1;
                }
            }
        }
        if self.count == self.steps.len() {
            Ok(Async::Ready(Outcome::Many(self.outcomes.iter_mut().map(|o| o.take().unwrap()).collect())))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for ParallelOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        self.operation.write().progress = Progress::from_steps(self.steps.iter().map(|o| o.operation.read().progress.clone()).collect());
        Ok(())
    }

    fn execute(&mut self) -> Result<Outcome, RuntimeError> {
        let mut running_ops = vec![];

        for op in self.steps_sync.drain(..) {
            let out = self.engine.start_operation(op);
            running_ops.push(out);
        }

        let mut outcomes = Vec::with_capacity(running_ops.len());
        for rec in running_ops {
            // fail
            outcomes.push(rec.receive()?)
        }
        Ok(Outcome::Many(outcomes))
    }
}

unsafe impl Sync for ParallelOperation {}

unsafe impl Send for ParallelOperation {}
