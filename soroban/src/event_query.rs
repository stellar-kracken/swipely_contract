//! Contract event query helpers with stable, filterable payloads.

use soroban_sdk::{contracttype, Address, Env, String, Vec};

use crate::keys;

/// Schema version for deterministic event payloads.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// Maximum events returned per query call.
pub const MAX_QUERY_LIMIT: u32 = 100;

/// Maximum events stored in the replay log.
pub const MAX_LOG_SIZE: u32 = 1_000;

/// One persisted contract event in canonical replay shape.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventReplayEntry {
    pub event_id: u32,
    pub event_type: String,
    pub actor: Address,
    pub subject: String,
    pub value: i128,
    pub timestamp: u64,
    pub ordering_key: u64,
    pub schema_version: u32,
}

/// Paginated replay page (ascending `ordering_key`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventReplayPage {
    pub entries: Vec<EventReplayEntry>,
    pub total: u32,
    pub schema_version: u32,
}

/// Filters for querying recent contract events.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractEventFilter {
    /// When set, only events whose `event_type` matches are returned.
    pub event_type: Option<String>,
    /// When set, only events whose `subject` (asset code) matches are returned.
    pub asset_code: Option<String>,
    /// Return events with `ordering_key` >= this cursor. Use `0` from the start.
    pub from_ordering_key: u64,
    /// Maximum events to return (capped at [`MAX_QUERY_LIMIT`]).
    pub limit: u32,
}

/// Deterministic query result for off-chain consumers.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractEventQueryResult {
    pub events: Vec<ContractEventRecord>,
    pub total_matched: u32,
    pub has_more: bool,
    pub schema_version: u32,
}

/// Stable outward-facing event record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractEventRecord {
    pub event_id: u32,
    pub event_type: String,
    pub actor: Address,
    pub asset_code: String,
    pub value: i128,
    pub timestamp: u64,
    pub ordering_key: u64,
    pub schema_version: u32,
}

fn load_log(env: &Env) -> Vec<EventReplayEntry> {
    env.storage()
        .persistent()
        .get(&keys::EVENT_REPLAY_LOG)
        .unwrap_or_else(|| Vec::new(env))
}

fn to_record(entry: &EventReplayEntry) -> ContractEventRecord {
    ContractEventRecord {
        event_id: entry.event_id,
        event_type: entry.event_type.clone(),
        actor: entry.actor.clone(),
        asset_code: entry.subject.clone(),
        value: entry.value,
        timestamp: entry.timestamp,
        ordering_key: entry.ordering_key,
        schema_version: entry.schema_version,
    }
}

fn matches_filter(entry: &EventReplayEntry, filter: &ContractEventFilter) -> bool {
    if entry.ordering_key < filter.from_ordering_key {
        return false;
    }
    if let Some(ref event_type) = filter.event_type {
        if entry.event_type != *event_type {
            return false;
        }
    }
    if let Some(ref asset_code) = filter.asset_code {
        if entry.subject != *asset_code {
            return false;
        }
    }
    true
}

/// Append one event to the replay log (internal).
pub fn append_event(
    env: &Env,
    event_type: String,
    actor: Address,
    subject: String,
    value: i128,
) {
    let seq: u32 = env
        .storage()
        .instance()
        .get(&keys::EVENT_REPLAY_CTR)
        .unwrap_or(0u32)
        + 1;
    env.storage()
        .instance()
        .set(&keys::EVENT_REPLAY_CTR, &seq);

    let now = env.ledger().timestamp();
    let ordering_key = (now << 32) | (seq as u64);
    let entry = EventReplayEntry {
        event_id: seq,
        event_type,
        actor,
        subject,
        value,
        timestamp: now,
        ordering_key,
        schema_version: EVENT_SCHEMA_VERSION,
    };

    let mut log = load_log(env);
    log.push_back(entry);
    if log.len() > MAX_LOG_SIZE {
        let mut trimmed: Vec<EventReplayEntry> = Vec::new(env);
        for i in 1..log.len() {
            trimmed.push_back(log.get(i).unwrap());
        }
        log = trimmed;
    }
    env.storage()
        .persistent()
        .set(&keys::EVENT_REPLAY_LOG, &log);
}

/// Query recent contract events with optional type and asset filters.
///
/// Results are ordered by ascending `ordering_key` (oldest first within the page).
pub fn query_events(env: Env, filter: ContractEventFilter) -> ContractEventQueryResult {
    let limit = if filter.limit > MAX_QUERY_LIMIT {
        MAX_QUERY_LIMIT
    } else if filter.limit == 0 {
        50
    } else {
        filter.limit
    };

    let log = load_log(&env);
    let mut matched: Vec<ContractEventRecord> = Vec::new(&env);
    let mut total_matched: u32 = 0;

    for i in 0..log.len() {
        let entry = log.get(i).unwrap();
        if !matches_filter(&entry, &filter) {
            continue;
        }
        total_matched += 1;
        if matched.len() < limit {
            matched.push_back(to_record(&entry));
        }
    }

    ContractEventQueryResult {
        has_more: total_matched > matched.len(),
        events: matched,
        total_matched,
        schema_version: EVENT_SCHEMA_VERSION,
    }
}

/// Replay page helper (backward compatible with issue #296).
pub fn get_replay_page(env: Env, from_ordering_key: u64, limit: u32) -> EventReplayPage {
    if limit > MAX_QUERY_LIMIT {
        panic!("limit must not exceed 100");
    }
    let log = load_log(&env);
    let total = log.len();
    let mut page: Vec<EventReplayEntry> = Vec::new(&env);
    for i in 0..total {
        if page.len() >= limit {
            break;
        }
        let entry = log.get(i).unwrap();
        if entry.ordering_key >= from_ordering_key {
            page.push_back(entry);
        }
    }
    EventReplayPage {
        entries: page,
        total,
        schema_version: EVENT_SCHEMA_VERSION,
    }
}

/// Total number of entries in the replay log.
pub fn log_size(env: &Env) -> u32 {
    load_log(env).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    #[test]
    fn test_query_events_by_type_and_asset() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let actor = Address::generate(&env);

        append_event(
            &env,
            String::from_str(&env, "health_up"),
            actor.clone(),
            String::from_str(&env, "USDC"),
            85,
        );
        env.ledger().set_timestamp(1_000_100);
        append_event(
            &env,
            String::from_str(&env, "price_up"),
            actor.clone(),
            String::from_str(&env, "USDC"),
            1_000_000,
        );
        env.ledger().set_timestamp(1_000_200);
        append_event(
            &env,
            String::from_str(&env, "health_up"),
            actor,
            String::from_str(&env, "EURC"),
            90,
        );

        let health_usdc = query_events(
            env.clone(),
            ContractEventFilter {
                event_type: Some(String::from_str(&env, "health_up")),
                asset_code: Some(String::from_str(&env, "USDC")),
                from_ordering_key: 0,
                limit: 10,
            },
        );
        assert_eq!(health_usdc.total_matched, 1);
        assert_eq!(health_usdc.events.len(), 1);
        assert_eq!(
            health_usdc.events.get(0).unwrap().asset_code,
            String::from_str(&env, "USDC")
        );

        let all = query_events(
            env,
            ContractEventFilter {
                event_type: None,
                asset_code: None,
                from_ordering_key: 0,
                limit: 2,
            },
        );
        assert_eq!(all.total_matched, 3);
        assert_eq!(all.events.len(), 2);
        assert!(all.has_more);
    }
}
