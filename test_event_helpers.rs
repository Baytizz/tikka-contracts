use soroban_sdk::{Env, Address, Symbol, Val, IntoVal, vec};

pub fn assert_event(
    env: &Env,
    expected_contract: &Address,
    expected_topic: &str,
    expected_payload: Val,
) {
    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(&last.0, expected_contract);
    assert_eq!(last.1.get(0).unwrap(), Symbol::new(env, "tikka").into_val(env));
    assert_eq!(last.1.get(1).unwrap(), Symbol::new(env, expected_topic).into_val(env));
    assert_eq!(last.2, expected_payload);
}
