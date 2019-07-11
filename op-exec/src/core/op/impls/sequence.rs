use super::*;
use std::collections::VecDeque;
use std::iter::FromIterator;
use std::sync::Mutex;

#[derive(Debug)]
pub struct SequenceOperation {
    engine: EngineRef,
    operation: OperationRef,
    state: OperationState
}

struct SequenceState {
    pub steps: VecDeque<OperationRef>,
    pub outcomes: Vec<Outcome>
}

impl SequenceOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, steps: Vec<OperationRef>) -> SequenceOperation {
        println!("1");
//        panic!();
        let o = operation.clone();
        let mut op = operation.write();
        println!("2");

        if op.state.is_none() {
            let state = SequenceState {
                steps: VecDeque::from_iter(steps),
                outcomes: vec![]
            };

            let state  = Arc::new(Mutex::new(Box::new(state) as Box<dyn Any>));
            op.set_state(state.clone());

            SequenceOperation {
                engine,
                operation: o,
                state
            }
        } else {
            SequenceOperation {
                engine,
                operation: o,
                state: op.state_mut().unwrap().clone()
            }
        }
    }
}

impl OperationImpl for SequenceOperation {
    fn execute(&mut self) -> Result<Outcome, RuntimeError> {
        println!("executing sequence...");

        let mut state = self.state.lock().unwrap();
        let state = state.downcast_mut::<SequenceState>().unwrap();

        let step = state.steps.pop_front();

        if let Some(s) = step {
            self.engine.enqueue_nested_operation(s, self.operation.clone())
        }

        Ok(Outcome::Empty)
    }

    fn wake_up(&mut self, finished_op: OperationRef) -> WakeUpStatus {
        println!("Waking up sequence...");
        let mut state = self.state.lock().unwrap();
        let state = state.downcast_mut::<SequenceState>().unwrap();

        let child_res = finished_op.write().take_result().unwrap();

        if child_res.is_err() {
            return WakeUpStatus::Ready(Err(child_res.unwrap_err()))
        }

        state.outcomes.push(child_res.unwrap());

        let step = state.steps.pop_front();

        if let Some(s) = step {
            // deadlock
            self.engine.enqueue_nested_operation(s, self.operation.clone());
            println!("sequence not ready...");
            WakeUpStatus::NotReady
        } else {
            println!("sequence ready!");
            WakeUpStatus::Ready(Ok(Outcome::Many(state.outcomes.drain(..).collect())))
        }

    }
}

unsafe impl Sync for SequenceOperation {}

unsafe impl Send for SequenceOperation {}
