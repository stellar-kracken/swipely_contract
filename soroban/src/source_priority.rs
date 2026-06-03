//! Source Priority Resolution for Bridge Watch.
//!
//! Defines which source should win when multiple sources report conflicting
//! data. Each source address is assigned a numeric priority (lower number =
//! higher precedence). When conflicts arise, the source with the lowest
//! priority value wins. Ties are broken deterministically by comparing the
//! raw address bytes.

use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::keys;

/// A stored source-priority mapping.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePriorityEntry {
    pub source: Address,
    pub priority: u32,
    pub updated_at: u64,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourcePriorityKey {
    /// Priority value for a single source address.
    Priority(Address),
    /// List of all source addresses with configured priorities.
    AllSources,
}

// ── Core Functions ────────────────────────────────────────────────────────────

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    if *caller != admin {
        panic!("only admin can manage source priorities");
    }
}

/// Set or update the priority level for a source address.
///
/// Lower values indicate higher priority. Admin only.
pub fn set_source_priority(env: &Env, caller: &Address, source: &Address, priority: u32) {
    require_admin(env, caller);

    let now = env.ledger().timestamp();
    let entry = SourcePriorityEntry {
        source: source.clone(),
        priority,
        updated_at: now,
    };

    let key = SourcePriorityKey::Priority(source.clone());
    env.storage().persistent().set(&key, &entry);

    let all_key = SourcePriorityKey::AllSources;
    let mut all: Vec<Address> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut found = false;
    for addr in all.iter() {
        if &addr == source {
            found = true;
            break;
        }
    }

    if !found {
        all.push_back(source.clone());
        env.storage().persistent().set(&all_key, &all);
    }

    env.events()
        .publish((symbol_short!("src_pri"),), (source.clone(), priority));
}

/// Return the priority for a given source.
///
/// Sources without an explicit priority return `u32::MAX`.
pub fn get_source_priority(env: &Env, source: &Address) -> u32 {
    let key = SourcePriorityKey::Priority(source.clone());
    let entry: Option<SourcePriorityEntry> = env.storage().persistent().get(&key);
    match entry {
        Some(e) => e.priority,
        None => u32::MAX,
    }
}

/// Return all configured source priority entries.
pub fn get_all_source_priorities(env: &Env) -> Vec<SourcePriorityEntry> {
    let all_key = SourcePriorityKey::AllSources;
    let all: Vec<Address> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut result: Vec<SourcePriorityEntry> = Vec::new(env);
    for addr in all.iter() {
        let key = SourcePriorityKey::Priority(addr.clone());
        if let Some(entry) = env.storage().persistent().get::<_, SourcePriorityEntry>(&key) {
            result.push_back(entry);
        }
    }
    result
}

/// Resolve which source wins among a list of conflicting sources.
///
/// Returns the source with the lowest priority value. When two sources share
/// the same priority, the one whose address is lexicographically smaller (by
/// raw `to_string()` representation) wins, guaranteeing deterministic output.
///
/// Panics if the input list is empty.
pub fn resolve_priority(env: &Env, sources: Vec<Address>) -> Address {
    if sources.is_empty() {
        panic!("sources list must not be empty");
    }

    let mut best_source = sources.get(0).unwrap();
    let mut best_priority = get_source_priority(env, &best_source);

    for i in 1..sources.len() {
        let candidate = sources.get(i).unwrap();
        let candidate_priority = get_source_priority(env, &candidate);

        if candidate_priority < best_priority {
            best_source = candidate;
            best_priority = candidate_priority;
        } else if candidate_priority == best_priority {
            // Deterministic tie-break: compare raw address representations.
            // In Soroban, Address implements Ord, so we can rely on its
            // comparison for deterministic ordering.
            if candidate < best_source {
                best_source = candidate;
            }
        }
    }

    best_source
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&keys::ADMIN, &admin);
        env.ledger().set_timestamp(1_000_000);
        (env, admin)
    }

    #[test]
    fn test_set_and_get_priority() {
        let (env, admin) = setup();
        let source_a = Address::generate(&env);

        set_source_priority(&env, &admin, &source_a, 10);
        assert_eq!(get_source_priority(&env, &source_a), 10);
    }

    #[test]
    fn test_unknown_source_returns_max() {
        let (env, _admin) = setup();
        let unknown = Address::generate(&env);
        assert_eq!(get_source_priority(&env, &unknown), u32::MAX);
    }

    #[test]
    fn test_get_all_priorities() {
        let (env, admin) = setup();
        let source_a = Address::generate(&env);
        let source_b = Address::generate(&env);

        set_source_priority(&env, &admin, &source_a, 5);
        set_source_priority(&env, &admin, &source_b, 15);

        let all = get_all_source_priorities(&env);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_update_existing_priority() {
        let (env, admin) = setup();
        let source_a = Address::generate(&env);

        set_source_priority(&env, &admin, &source_a, 10);
        assert_eq!(get_source_priority(&env, &source_a), 10);

        set_source_priority(&env, &admin, &source_a, 3);
        assert_eq!(get_source_priority(&env, &source_a), 3);

        // Should still only have one entry in the list
        let all = get_all_source_priorities(&env);
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_resolve_priority_single_source() {
        let (env, admin) = setup();
        let source_a = Address::generate(&env);
        set_source_priority(&env, &admin, &source_a, 1);

        let mut sources = Vec::new(&env);
        sources.push_back(source_a.clone());

        let winner = resolve_priority(&env, sources);
        assert_eq!(winner, source_a);
    }

    #[test]
    fn test_resolve_priority_multiple_sources() {
        let (env, admin) = setup();
        let source_a = Address::generate(&env);
        let source_b = Address::generate(&env);
        let source_c = Address::generate(&env);

        set_source_priority(&env, &admin, &source_a, 20);
        set_source_priority(&env, &admin, &source_b, 5);
        set_source_priority(&env, &admin, &source_c, 10);

        let mut sources = Vec::new(&env);
        sources.push_back(source_a);
        sources.push_back(source_b.clone());
        sources.push_back(source_c);

        let winner = resolve_priority(&env, sources);
        assert_eq!(winner, source_b);
    }

    #[test]
    fn test_resolve_priority_unknown_sources_lose() {
        let (env, admin) = setup();
        let known = Address::generate(&env);
        let unknown = Address::generate(&env);

        set_source_priority(&env, &admin, &known, 50);

        let mut sources = Vec::new(&env);
        sources.push_back(unknown);
        sources.push_back(known.clone());

        let winner = resolve_priority(&env, sources);
        assert_eq!(winner, known);
    }

    #[test]
    fn test_resolve_priority_deterministic_tiebreak() {
        let (env, admin) = setup();
        let source_a = Address::generate(&env);
        let source_b = Address::generate(&env);

        // Same priority
        set_source_priority(&env, &admin, &source_a, 10);
        set_source_priority(&env, &admin, &source_b, 10);

        let mut sources_ab = Vec::new(&env);
        sources_ab.push_back(source_a.clone());
        sources_ab.push_back(source_b.clone());

        let mut sources_ba = Vec::new(&env);
        sources_ba.push_back(source_b);
        sources_ba.push_back(source_a);

        let winner_ab = resolve_priority(&env, sources_ab);
        let winner_ba = resolve_priority(&env, sources_ba);

        // Same winner regardless of input order
        assert_eq!(winner_ab, winner_ba);
    }

    #[test]
    #[should_panic(expected = "sources list must not be empty")]
    fn test_resolve_priority_empty_panics() {
        let (env, _admin) = setup();
        let sources: Vec<Address> = Vec::new(&env);
        resolve_priority(&env, sources);
    }

    #[test]
    #[should_panic(expected = "only admin")]
    fn test_non_admin_cannot_set_priority() {
        let (env, _admin) = setup();
        let stranger = Address::generate(&env);
        let source = Address::generate(&env);
        set_source_priority(&env, &stranger, &source, 1);
    }
}
