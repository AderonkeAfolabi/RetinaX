//! Break-glass audit report (#448): a patient lists every emergency access to their data.

extern crate std;

use std::{string::ToString, vec::Vec as StdVec};

use crate::{
    emergency::{self, EmergencyAccessReport, EmergencyCondition, EmergencyStatus},
    provider::{self, Provider, VerificationStatus},
    VisionRecordsContract, VisionRecordsContractClient,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String, Vec,
};

const DURATION: u64 = 3600;

fn setup() -> (Env, VisionRecordsContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let client = VisionRecordsContractClient::new(&env, &env.register(VisionRecordsContract, ()));
    client.initialize(&Address::generate(&env));
    env.ledger().set_timestamp(1_000);
    (env, client)
}

/// Registers a verified provider, the only kind of requester that can break the glass.
fn verified_provider(env: &Env, client: &VisionRecordsContractClient) -> Address {
    let address = Address::generate(env);
    env.as_contract(&client.address, || {
        provider::set_provider(
            env,
            &Provider {
                address: address.clone(),
                name: String::from_str(env, "ER"),
                licenses: Vec::new(env),
                specialties: Vec::new(env),
                certifications: Vec::new(env),
                locations: Vec::new(env),
                verification_status: VerificationStatus::Verified,
                registered_at: 0,
                verified_at: None,
                verified_by: None,
                is_active: true,
            },
        )
    });
    address
}

fn grant(
    env: &Env,
    client: &VisionRecordsContractClient,
    requester: &Address,
    patient: &Address,
    condition: EmergencyCondition,
) -> u64 {
    env.as_contract(&client.address, || {
        emergency::grant_emergency_access(
            env,
            requester,
            patient,
            condition,
            String::from_str(env, "immediate care needed"),
            DURATION,
            Vec::new(env),
        )
    })
    .unwrap()
}

/// `(access_id, actor, action, timestamp)` of each audit entry in a report item.
fn trail(item: &EmergencyAccessReport) -> StdVec<(u64, Address, std::string::String, u64)> {
    item.audit_trail
        .iter()
        .map(|e| (e.access_id, e.actor, e.action.to_string(), e.timestamp))
        .collect()
}

#[test]
fn report_lists_every_accessor_with_their_audit_trail() {
    let (env, client) = setup();
    let patient = Address::generate(&env);
    let surgeon = verified_provider(&env, &client);
    let paramedic = verified_provider(&env, &client);

    let first = grant(
        &env,
        &client,
        &surgeon,
        &patient,
        EmergencyCondition::SurgicalEmergency,
    );
    env.ledger().set_timestamp(1_100);
    let second = grant(
        &env,
        &client,
        &paramedic,
        &patient,
        EmergencyCondition::Unconscious,
    );
    env.ledger().set_timestamp(1_200);
    client.access_record_via_emergency(&paramedic, &patient, &Some(7));
    env.ledger().set_timestamp(1_300);
    client.access_record_via_emergency(&surgeon, &patient, &None);
    client.access_record_via_emergency(&paramedic, &patient, &None);

    let report = client.get_emergency_access_report(&patient);
    assert_eq!(report.len(), 2);

    let item = report.get_unchecked(0);
    assert_eq!(item.access.id, first);
    assert_eq!(item.access.requester, surgeon);
    assert_eq!(item.access.condition, EmergencyCondition::SurgicalEmergency);
    assert_eq!(
        trail(&item),
        std::vec![
            (first, surgeon.clone(), "GRANTED".to_string(), 1_000),
            (first, surgeon, "ACCESSED".to_string(), 1_300),
        ]
    );

    let item = report.get_unchecked(1);
    assert_eq!(item.access.id, second);
    assert_eq!(item.access.requester, paramedic);
    assert_eq!(item.access.condition, EmergencyCondition::Unconscious);
    assert_eq!(
        trail(&item),
        std::vec![
            (second, paramedic.clone(), "GRANTED".to_string(), 1_100),
            (second, paramedic.clone(), "ACCESSED".to_string(), 1_200),
            (second, paramedic, "ACCESSED".to_string(), 1_300),
        ]
    );
}

#[test]
fn report_keeps_ended_accesses_and_repeat_requesters() {
    let (env, client) = setup();
    let patient = Address::generate(&env);
    let doctor = verified_provider(&env, &client);
    let condition = EmergencyCondition::LifeThreatening;

    let revoked = grant(&env, &client, &doctor, &patient, condition.clone());
    env.as_contract(&client.address, || {
        emergency::revoke_emergency_access(&env, &patient, revoked)
    })
    .unwrap();

    let expired = grant(&env, &client, &doctor, &patient, condition.clone());
    env.ledger().set_timestamp(1_000 + DURATION);
    env.as_contract(&client.address, || {
        emergency::expire_emergency_accesses(&env)
    });

    let active = grant(&env, &client, &doctor, &patient, condition);

    let report = client.get_emergency_access_report(&patient);
    let listed: StdVec<_> = report
        .iter()
        .map(|item| (item.access.id, item.access.status))
        .collect();
    assert_eq!(
        listed,
        std::vec![
            (revoked, EmergencyStatus::Revoked),
            (expired, EmergencyStatus::Expired),
            (active, EmergencyStatus::Active),
        ]
    );
    assert_eq!(
        trail(&report.get_unchecked(0))[1],
        (revoked, patient.clone(), "REVOKED".to_string(), 1_000)
    );
    assert_eq!(trail(&report.get_unchecked(1))[1].2, "EXPIRED");
}

#[test]
fn report_only_covers_the_requested_patient() {
    let (env, client) = setup();
    let patient = Address::generate(&env);
    let other = Address::generate(&env);
    let doctor = verified_provider(&env, &client);
    let condition = EmergencyCondition::Unconscious;

    grant(&env, &client, &doctor, &other, condition.clone());
    let own = grant(&env, &client, &doctor, &patient, condition);

    let report = client.get_emergency_access_report(&patient);
    assert_eq!(report.len(), 1);
    assert_eq!(report.get_unchecked(0).access.id, own);

    let untouched = Address::generate(&env);
    assert!(client.get_emergency_access_report(&untouched).is_empty());
}

/// The other emergency queries only look at the last 100 grants of the whole contract.
#[test]
fn report_includes_accesses_older_than_the_last_100_grants() {
    let (env, client) = setup();
    let patient = Address::generate(&env);
    let other = Address::generate(&env);
    let doctor = verified_provider(&env, &client);
    let condition = EmergencyCondition::Masscasualties;

    let oldest = grant(&env, &client, &doctor, &patient, condition.clone());
    for _ in 0..101 {
        grant(&env, &client, &doctor, &other, condition.clone());
    }

    let report = client.get_emergency_access_report(&patient);
    assert_eq!(report.len(), 1);
    assert_eq!(report.get_unchecked(0).access.id, oldest);
}

#[test]
fn report_requires_the_patients_authorization() {
    let (env, client) = setup();
    let patient = Address::generate(&env);
    let doctor = verified_provider(&env, &client);
    grant(
        &env,
        &client,
        &doctor,
        &patient,
        EmergencyCondition::Unconscious,
    );

    client.get_emergency_access_report(&patient);
    assert_eq!(env.auths()[0].0, patient);

    env.set_auths(&[]);
    assert!(client.try_get_emergency_access_report(&patient).is_err());
}
