use crate::outcome::Outcome;
use op_engine::OperationRef;

mod parallel;
mod sequence;

fn handle_cancel(ops: Vec<OperationRef<Outcome>>, operation: &OperationRef<Outcome>) {
    let mut cancel_rx = operation.write().take_cancel_receiver().unwrap();
    tokio::spawn(async move {
        if cancel_rx.recv().await.is_some() {
            let mut futs = Vec::with_capacity(ops.len());
            for op in ops.iter() {
                futs.push(op.cancel())
            }
            futures::future::join_all(futs).await;
        }
    });
}
