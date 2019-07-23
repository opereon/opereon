use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    uid: u32,
    username: String,
    groups: Vec<Group>,
}

impl User {}
