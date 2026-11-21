use common::transaction::{ContractType, TransactionError};
use soroban_sdk::{contracttype, symbol_short, Env, String, Symbol, Vec};

const TEMPLATE: Symbol = symbol_short!("WF_TPL");
const LATEST: Symbol = symbol_short!("WF_LATE");
const DEPLOYED: Symbol = symbol_short!("WF_LIVE");

/// A reusable operation description. Contract addresses and parameters are
/// supplied by the transaction initiator when the workflow is executed.
#[contracttype]
#[derive(Clone, Debug)]
pub struct WorkflowStep {
    pub contract_type: ContractType,
    pub function_name: String,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct WorkflowTemplate {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub steps: Vec<WorkflowStep>,
    pub built_in: bool,
}

pub fn validate(template: &WorkflowTemplate) -> Result<(), TransactionError> {
    if template.id.is_empty()
        || template.id.len() > 64
        || template.name.is_empty()
        || template.name.len() > 128
        || template.version == 0
        || template.steps.is_empty()
        || template.steps.len() > 50
    {
        return Err(TransactionError::InvalidInput);
    }
    for step in template.steps.iter() {
        super::validation::validate_function_name(&step.function_name)?;
    }
    Ok(())
}

pub fn save(env: &Env, template: &WorkflowTemplate) {
    env.storage()
        .persistent()
        .set(&(TEMPLATE, template.id.clone(), template.version), template);
    env.storage()
        .persistent()
        .set(&(LATEST, template.id.clone()), &template.version);
}

pub fn get(env: &Env, id: &String, version: u32) -> Option<WorkflowTemplate> {
    env.storage()
        .persistent()
        .get(&(TEMPLATE, id.clone(), version))
}

pub fn latest(env: &Env, id: &String) -> Option<u32> {
    env.storage().persistent().get(&(LATEST, id.clone()))
}

pub fn deployed(env: &Env, id: &String) -> Option<u32> {
    env.storage().persistent().get(&(DEPLOYED, id.clone()))
}

pub fn deploy(env: &Env, id: &String, version: u32) {
    env.storage()
        .persistent()
        .set(&(DEPLOYED, id.clone()), &version);
}

fn built_in(env: &Env, id: &str, name: &str, functions: &[(ContractType, &str)]) {
    let mut steps = Vec::new(env);
    for (contract_type, function_name) in functions {
        steps.push_back(WorkflowStep {
            contract_type: (*contract_type).clone(),
            function_name: String::from_str(env, function_name),
        });
    }
    let template = WorkflowTemplate {
        id: String::from_str(env, id),
        name: String::from_str(env, name),
        version: 1,
        steps,
        built_in: true,
    };
    save(env, &template);
    deploy(env, &template.id, 1);
}

pub fn install_built_ins(env: &Env) {
    built_in(
        env,
        "patient_registration",
        "Patient registration",
        &[
            (ContractType::Identity, "create_did"),
            (ContractType::VisionRecords, "register_user"),
        ],
    );
    built_in(
        env,
        "examination",
        "Examination",
        &[
            (ContractType::VisionRecords, "add_eye_examination"),
            (ContractType::VisionRecords, "add_record"),
        ],
    );
    built_in(
        env,
        "referral",
        "Referral",
        &[(ContractType::VisionRecords, "grant_access")],
    );
}
