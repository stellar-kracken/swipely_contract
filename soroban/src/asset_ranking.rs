//! Asset Priority Ranking for Bridge Watch.
//!
//! Computes a deterministic ranking for monitored assets based on configurable
//! weighted factors. The ranking guides display order and monitoring priority.
//! Assets with equal scores are ordered alphabetically by asset code for
//! stable, reproducible output.

use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::keys;

/// Default weight for the health component (out of 100).
pub const DEFAULT_HEALTH_WEIGHT: u32 = 40;
/// Default weight for the volume/price-stability component (out of 100).
pub const DEFAULT_VOLUME_WEIGHT: u32 = 30;
/// Default weight for the liquidity component (out of 100).
pub const DEFAULT_LIQUIDITY_WEIGHT: u32 = 30;

/// Configurable weights for asset ranking calculation.
///
/// Each weight is 0-100 and all three must sum to exactly 100.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RankingWeights {
    pub health_weight: u32,
    pub volume_weight: u32,
    pub liquidity_weight: u32,
    pub version: u32,
}

/// A computed rank for a single asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRank {
    pub asset_code: String,
    pub rank: u32,
    pub score: u32,
    pub timestamp: u64,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetRankingKey {
    /// Stored ranking weights configuration.
    Weights,
}

// ── Internal Helpers ──────────────────────────────────────────────────────────

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    if *caller != admin {
        panic!("only admin can update ranking weights");
    }
}

/// Load ranking weights, returning defaults if none configured.
pub fn get_ranking_weights(env: &Env) -> RankingWeights {
    env.storage()
        .persistent()
        .get(&AssetRankingKey::Weights)
        .unwrap_or(RankingWeights {
            health_weight: DEFAULT_HEALTH_WEIGHT,
            volume_weight: DEFAULT_VOLUME_WEIGHT,
            liquidity_weight: DEFAULT_LIQUIDITY_WEIGHT,
            version: 1,
        })
}

// ── Core Functions ────────────────────────────────────────────────────────────

/// Set ranking weights. Admin only.
///
/// Weights must each be 0-100 and sum to exactly 100.
pub fn set_ranking_weights(
    env: &Env,
    caller: &Address,
    health_weight: u32,
    volume_weight: u32,
    liquidity_weight: u32,
) {
    require_admin(env, caller);

    if health_weight + volume_weight + liquidity_weight != 100 {
        panic!("ranking weights must sum to 100");
    }

    let current = get_ranking_weights(env);
    let weights = RankingWeights {
        health_weight,
        volume_weight,
        liquidity_weight,
        version: current.version + 1,
    };

    env.storage()
        .persistent()
        .set(&AssetRankingKey::Weights, &weights);

    env.events().publish(
        (symbol_short!("rnk_wt"),),
        (health_weight, volume_weight, liquidity_weight),
    );
}

/// Compute the ranking score for a single asset.
///
/// The score is calculated as:
///   score = (health_score * health_weight
///          + price_stability_score * volume_weight
///          + liquidity_score * liquidity_weight) / 100
///
/// Each component score is 0-100, so the result is also 0-100.
///
/// Read-only.
pub fn compute_asset_rank(
    env: &Env,
    asset_code: String,
    health_score: u32,
    price_stability_score: u32,
    liquidity_score: u32,
) -> AssetRank {
    let weights = get_ranking_weights(env);
    let now = env.ledger().timestamp();

    let score = (health_score * weights.health_weight
        + price_stability_score * weights.volume_weight
        + liquidity_score * weights.liquidity_weight)
        / 100;

    AssetRank {
        asset_code,
        rank: 0, // rank is assigned when computing all rankings
        score,
        timestamp: now,
    }
}

/// Compute rankings for a batch of assets.
///
/// Accepts pre-collected scores for each asset, computes weighted scores,
/// sorts descending by score, and assigns rank numbers starting from 1.
/// Assets with equal scores are ordered alphabetically by asset code.
///
/// Read-only. Deterministic: same inputs always produce the same output.
pub fn compute_all_rankings(
    env: &Env,
    assets: Vec<AssetScoreInput>,
) -> Vec<AssetRank> {
    let weights = get_ranking_weights(env);
    let now = env.ledger().timestamp();

    // Compute scores for all assets
    let mut scored: Vec<AssetRank> = Vec::new(env);
    for input in assets.iter() {
        let score = (input.health_score * weights.health_weight
            + input.price_stability_score * weights.volume_weight
            + input.liquidity_score * weights.liquidity_weight)
            / 100;

        scored.push_back(AssetRank {
            asset_code: input.asset_code.clone(),
            rank: 0,
            score,
            timestamp: now,
        });
    }

    // Sort: descending by score, then ascending alphabetically by asset_code
    // Using insertion sort since Soroban Vec does not have a built-in sort
    let len = scored.len();
    for i in 1..len {
        let current = scored.get(i).unwrap();
        let mut j = i;
        while j > 0 {
            let prev = scored.get(j - 1).unwrap();
            let should_swap = if current.score > prev.score {
                true
            } else if current.score == prev.score {
                // Alphabetical tie-break: compare asset codes character by character
                current.asset_code < prev.asset_code
            } else {
                false
            };

            if should_swap {
                scored.set(j, prev);
                j -= 1;
            } else {
                break;
            }
        }
        scored.set(j, current);
    }

    // Assign rank numbers
    let mut ranked: Vec<AssetRank> = Vec::new(env);
    for i in 0..scored.len() {
        let mut entry = scored.get(i).unwrap();
        entry.rank = i + 1;
        ranked.push_back(entry);
    }

    ranked
}

/// Input structure for batch ranking computation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetScoreInput {
    pub asset_code: String,
    pub health_score: u32,
    pub price_stability_score: u32,
    pub liquidity_score: u32,
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
    fn test_default_weights() {
        let (env, _admin) = setup();
        let weights = get_ranking_weights(&env);
        assert_eq!(weights.health_weight, 40);
        assert_eq!(weights.volume_weight, 30);
        assert_eq!(weights.liquidity_weight, 30);
        assert_eq!(weights.version, 1);
    }

    #[test]
    fn test_set_weights() {
        let (env, admin) = setup();
        set_ranking_weights(&env, &admin, 50, 25, 25);

        let weights = get_ranking_weights(&env);
        assert_eq!(weights.health_weight, 50);
        assert_eq!(weights.volume_weight, 25);
        assert_eq!(weights.liquidity_weight, 25);
        assert_eq!(weights.version, 2);
    }

    #[test]
    #[should_panic(expected = "ranking weights must sum to 100")]
    fn test_weights_must_sum_to_100() {
        let (env, admin) = setup();
        set_ranking_weights(&env, &admin, 50, 30, 30);
    }

    #[test]
    fn test_compute_single_asset_rank() {
        let (env, _admin) = setup();
        let asset = String::from_str(&env, "USDC");

        // Default weights: health 40, volume 30, liquidity 30
        // Score = (80*40 + 90*30 + 70*30) / 100 = (3200+2700+2100)/100 = 80
        let rank = compute_asset_rank(&env, asset, 80, 90, 70);
        assert_eq!(rank.score, 80);
    }

    #[test]
    fn test_compute_all_rankings_sorted() {
        let (env, _admin) = setup();

        let mut inputs: Vec<AssetScoreInput> = Vec::new(&env);
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "XLM"),
            health_score: 60,
            price_stability_score: 70,
            liquidity_score: 50,
        });
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "USDC"),
            health_score: 90,
            price_stability_score: 85,
            liquidity_score: 80,
        });
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "EURC"),
            health_score: 75,
            price_stability_score: 80,
            liquidity_score: 70,
        });

        let rankings = compute_all_rankings(&env, inputs);

        assert_eq!(rankings.len(), 3);
        // USDC should be rank 1 (highest score)
        assert_eq!(rankings.get(0).unwrap().asset_code, String::from_str(&env, "USDC"));
        assert_eq!(rankings.get(0).unwrap().rank, 1);
        // EURC should be rank 2
        assert_eq!(rankings.get(1).unwrap().asset_code, String::from_str(&env, "EURC"));
        assert_eq!(rankings.get(1).unwrap().rank, 2);
        // XLM should be rank 3 (lowest score)
        assert_eq!(rankings.get(2).unwrap().asset_code, String::from_str(&env, "XLM"));
        assert_eq!(rankings.get(2).unwrap().rank, 3);
    }

    #[test]
    fn test_stable_ordering_equal_scores() {
        let (env, _admin) = setup();

        let mut inputs: Vec<AssetScoreInput> = Vec::new(&env);
        // Both have score = (80*40 + 80*30 + 80*30)/100 = 80
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "USDC"),
            health_score: 80,
            price_stability_score: 80,
            liquidity_score: 80,
        });
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "EURC"),
            health_score: 80,
            price_stability_score: 80,
            liquidity_score: 80,
        });

        let rankings = compute_all_rankings(&env, inputs.clone());

        // EURC < USDC alphabetically, so EURC comes first with equal scores
        assert_eq!(rankings.get(0).unwrap().asset_code, String::from_str(&env, "EURC"));
        assert_eq!(rankings.get(1).unwrap().asset_code, String::from_str(&env, "USDC"));

        // Reverse input order, result should be the same
        let mut inputs_reversed: Vec<AssetScoreInput> = Vec::new(&env);
        inputs_reversed.push_back(inputs.get(1).unwrap());
        inputs_reversed.push_back(inputs.get(0).unwrap());

        let rankings_reversed = compute_all_rankings(&env, inputs_reversed);
        assert_eq!(
            rankings_reversed.get(0).unwrap().asset_code,
            String::from_str(&env, "EURC")
        );
        assert_eq!(
            rankings_reversed.get(1).unwrap().asset_code,
            String::from_str(&env, "USDC")
        );
    }

    #[test]
    fn test_custom_weights_affect_ranking() {
        let (env, admin) = setup();
        // Set weights: health 100, volume 0, liquidity 0
        set_ranking_weights(&env, &admin, 100, 0, 0);

        let mut inputs: Vec<AssetScoreInput> = Vec::new(&env);
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "USDC"),
            health_score: 50,
            price_stability_score: 100,
            liquidity_score: 100,
        });
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "XLM"),
            health_score: 90,
            price_stability_score: 10,
            liquidity_score: 10,
        });

        let rankings = compute_all_rankings(&env, inputs);

        // With 100% health weight, XLM (health=90) beats USDC (health=50)
        assert_eq!(rankings.get(0).unwrap().asset_code, String::from_str(&env, "XLM"));
        assert_eq!(rankings.get(0).unwrap().score, 90);
        assert_eq!(rankings.get(1).unwrap().asset_code, String::from_str(&env, "USDC"));
        assert_eq!(rankings.get(1).unwrap().score, 50);
    }

    #[test]
    fn test_empty_inputs_returns_empty() {
        let (env, _admin) = setup();
        let inputs: Vec<AssetScoreInput> = Vec::new(&env);
        let rankings = compute_all_rankings(&env, inputs);
        assert_eq!(rankings.len(), 0);
    }

    #[test]
    fn test_single_asset_gets_rank_1() {
        let (env, _admin) = setup();
        let mut inputs: Vec<AssetScoreInput> = Vec::new(&env);
        inputs.push_back(AssetScoreInput {
            asset_code: String::from_str(&env, "USDC"),
            health_score: 80,
            price_stability_score: 80,
            liquidity_score: 80,
        });

        let rankings = compute_all_rankings(&env, inputs);
        assert_eq!(rankings.len(), 1);
        assert_eq!(rankings.get(0).unwrap().rank, 1);
    }

    #[test]
    #[should_panic(expected = "only admin")]
    fn test_non_admin_cannot_set_weights() {
        let (env, _admin) = setup();
        let stranger = Address::generate(&env);
        set_ranking_weights(&env, &stranger, 40, 30, 30);
    }
}
