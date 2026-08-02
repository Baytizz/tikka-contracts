use soroban_sdk::{contracttype, contractevent, Address, Env};

#[contracttype]
#[contractevent]
pub struct TestEvent {
    pub a: u32,
}
