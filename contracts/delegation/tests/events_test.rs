use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    vec, Address, Env, IntoVal,
};
use retinax_delegation::{DelegationContract, DelegationContractClient};

#[test]
fn initialize_publishes_admin_event() {
    let env = Env::default();
    let contract_id = env.register(DelegationContract, ());
    let client = DelegationContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let expected = vec![
        &env,
        (
            contract_id,
            vec![&env, symbol_short!("INIT").into_val(&env)],
            admin.into_val(&env),
        ),
    ];
    assert_eq!(env.events().all(), expected);
}
