use op_exec::{Context, Outcome, RuntimeError};
use daemon::InstanceInfo;
#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Outcome(Outcome),
    Error(String),
    ReachableInstances(Vec<InstanceInfo>),
    Progress,
}
#[derive(Debug, Deserialize, Serialize)]
pub enum CliMessage {
    Execute(Context),
    GetReachableInstances,
    Cancel,
}

impl From<Context> for CliMessage {
    fn from(ctx: Context) -> Self {
        CliMessage::Execute(ctx)
    }
}
