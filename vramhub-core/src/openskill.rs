// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! PlackettLuce OpenSkill rating system.
//! Rust port of the Python `openskill` library used in Templar/Gauntlet.
//!
//! In v0.4, the OpenSkill update runs INSIDE the Nautilus enclave
//! (called from vramhub-nautilus/src/evaluator.rs).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSkillRating {
    pub mu: f64,
    pub sigma: f64,
}

impl Default for OpenSkillRating {
    fn default() -> Self {
        Self {
            mu: 25.0,
            sigma: 25.0 / 3.0,
        }
    }
}

impl OpenSkillRating {
    pub fn ordinal(&self) -> f64 {
        self.mu - 3.0 * self.sigma
    }
}

pub fn update_ratings(ratings: &mut [OpenSkillRating], ranking: &[usize], beta: f64, tau: f64) {
    let n = ratings.len();
    if n == 0 || ranking.len() != n {
        return;
    }

    for r in ratings.iter_mut() {
        r.sigma = (r.sigma.powi(2) + tau.powi(2)).sqrt();
    }

    // c² = Σᵢ(σᵢ² + β²)  — standard Plackett-Luce formula (one β² per player)
    let c = ratings
        .iter()
        .map(|r| r.sigma.powi(2) + beta.powi(2))
        .sum::<f64>()
        .sqrt();

    let mut omegas = vec![0.0f64; n];
    let mut deltas = vec![0.0f64; n];

    // Plackett-Luce gradient: for player i at rank k, sum the probability term
    // over all rank positions q from 0 to k (inclusive).
    //   omega_i = (1 - sum_{q=0}^{k} exp(mu_i/c) / A_q) / c
    //   A_q     = sum of exp(mu_j/c) for all players at rank >= q
    // This correctly gives positive omega for winners and negative for losers.
    for i in 0..n {
        let rank_i = ranking.iter().position(|&j| j == i).unwrap();
        let exp_i = (ratings[i].mu / c).exp();
        let mut omega_sum = 0.0f64;
        let mut delta_sum = 0.0f64;

        for q in 0..=rank_i {
            let a_q: f64 = ranking[q..]
                .iter()
                .map(|&j| (ratings[j].mu / c).exp())
                .sum();
            let term = exp_i / a_q;
            omega_sum += term;
            delta_sum += term * (1.0 - term);
        }

        omegas[i] = (1.0 - omega_sum) / c;
        deltas[i] = delta_sum / c.powi(2);
    }

    for i in 0..n {
        let sigma_sq = ratings[i].sigma.powi(2);
        ratings[i].mu += sigma_sq * omegas[i];
        ratings[i].sigma = (sigma_sq * (1.0 - sigma_sq * deltas[i])).sqrt();
        ratings[i].sigma = ratings[i].sigma.max(tau);
    }
}

pub fn normalized_weights(ratings: &[OpenSkillRating]) -> Vec<f64> {
    let ordinals: Vec<f64> = ratings.iter().map(|r| r.ordinal()).collect();
    let min_ord = ordinals.iter().cloned().fold(f64::INFINITY, f64::min);
    let shifted: Vec<f64> = ordinals.iter().map(|&o| o - min_ord).collect();
    let powered: Vec<f64> = shifted.iter().map(|&s| s.powi(2)).collect();
    let total: f64 = powered.iter().sum();
    if total == 0.0 {
        return vec![1.0 / ratings.len() as f64; ratings.len()];
    }
    powered.iter().map(|&p| p / total).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_rating() -> OpenSkillRating {
        OpenSkillRating::default()
    }

    #[test]
    fn default_rating_values() {
        let r = default_rating();
        assert!((r.mu - 25.0).abs() < 1e-9);
        assert!((r.sigma - 25.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn ordinal_is_mu_minus_3_sigma() {
        let r = OpenSkillRating {
            mu: 25.0,
            sigma: 8.333,
        };
        let expected = 25.0 - 3.0 * 8.333;
        assert!((r.ordinal() - expected).abs() < 1e-6);
    }

    #[test]
    fn update_ratings_two_players_winner_gains() {
        let mut ratings = vec![
            OpenSkillRating {
                mu: 25.0,
                sigma: 8.333,
            },
            OpenSkillRating {
                mu: 25.0,
                sigma: 8.333,
            },
        ];
        let ranking = vec![0, 1]; // player 0 ranked first
        update_ratings(&mut ratings, &ranking, 25.0 / 6.0, 25.0 / 300.0);
        // Winner's mu should increase, loser's should decrease
        assert!(ratings[0].mu > 25.0, "winner mu should increase");
        assert!(ratings[1].mu < 25.0, "loser mu should decrease");
        // Both sigmas should decrease (more information)
        assert!(
            ratings[0].sigma < 8.333,
            "sigma should decrease after match"
        );
        assert!(
            ratings[1].sigma < 8.333,
            "sigma should decrease after match"
        );
    }

    #[test]
    fn update_ratings_empty_is_noop() {
        let mut ratings: Vec<OpenSkillRating> = vec![];
        update_ratings(&mut ratings, &[], 4.166, 0.083);
        assert!(ratings.is_empty());
    }

    #[test]
    fn update_ratings_mismatched_lengths_is_noop() {
        let mut ratings = vec![OpenSkillRating::default()];
        update_ratings(&mut ratings, &[0, 1], 4.166, 0.083); // ranking longer than ratings
                                                             // Should not panic, ratings unchanged
        assert!((ratings[0].mu - 25.0).abs() < 1.0);
    }

    #[test]
    fn update_ratings_single_player() {
        let mut ratings = vec![OpenSkillRating::default()];
        let ranking = vec![0];
        let mu_before = ratings[0].mu;
        update_ratings(&mut ratings, &ranking, 4.166, 0.083);
        // Single player: ranking doesn't change mu much, but sigma increases slightly then decreases
        let _ = mu_before; // just check it doesn't panic
    }

    #[test]
    fn normalized_weights_equal_ratings_are_uniform() {
        let ratings = vec![
            OpenSkillRating {
                mu: 25.0,
                sigma: 8.333,
            },
            OpenSkillRating {
                mu: 25.0,
                sigma: 8.333,
            },
            OpenSkillRating {
                mu: 25.0,
                sigma: 8.333,
            },
        ];
        let weights = normalized_weights(&ratings);
        assert_eq!(weights.len(), 3);
        for w in &weights {
            assert!(
                (w - 1.0 / 3.0).abs() < 1e-9,
                "equal ratings should have equal weights"
            );
        }
    }

    #[test]
    fn normalized_weights_sum_to_one() {
        let mut ratings = vec![
            OpenSkillRating {
                mu: 30.0,
                sigma: 5.0,
            },
            OpenSkillRating {
                mu: 20.0,
                sigma: 8.0,
            },
            OpenSkillRating {
                mu: 25.0,
                sigma: 6.0,
            },
        ];
        let ranking = vec![0, 1, 2];
        update_ratings(&mut ratings, &ranking, 4.166, 0.083);
        let weights = normalized_weights(&ratings);
        let sum: f64 = weights.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-9,
            "weights must sum to 1.0, got {sum}"
        );
    }

    #[test]
    fn normalized_weights_higher_ordinal_gets_more_weight() {
        let ratings = vec![
            OpenSkillRating {
                mu: 30.0,
                sigma: 3.0,
            }, // ordinal = 21.0
            OpenSkillRating {
                mu: 20.0,
                sigma: 3.0,
            }, // ordinal = 11.0
        ];
        let weights = normalized_weights(&ratings);
        assert!(
            weights[0] > weights[1],
            "higher ordinal should get more weight"
        );
    }

    #[test]
    fn update_then_weights_top_ranked_leads() {
        let mut ratings = vec![
            OpenSkillRating::default(),
            OpenSkillRating::default(),
            OpenSkillRating::default(),
        ];
        // Player 0 wins every round
        for _ in 0..10 {
            let ranking = vec![0, 1, 2];
            update_ratings(&mut ratings, &ranking, 4.166, 0.083);
        }
        let weights = normalized_weights(&ratings);
        assert!(
            weights[0] > weights[1],
            "consistent winner should have highest weight"
        );
        assert!(weights[1] > weights[2], "middle should beat bottom");
    }

    #[test]
    fn c_factor_does_not_include_extra_beta_n_term() {
        // With the buggy formula c² = Σ(σ²+β²) + β²×n the mu update for a
        // 2-player match at default ratings is ≈10% smaller than the correct
        // value. Pin the expected delta to catch regressions.
        let beta = 25.0 / 6.0;
        let mut ratings = vec![
            OpenSkillRating {
                mu: 25.0,
                sigma: 25.0 / 3.0,
            },
            OpenSkillRating {
                mu: 25.0,
                sigma: 25.0 / 3.0,
            },
        ];
        update_ratings(&mut ratings, &[0, 1], beta, 25.0 / 300.0);

        // Correct c² = 2*(σ²+β²) ≈ 173.6, c ≈ 13.17  →  mu delta ≈ 2.64
        // Buggy c² adds β²×n extra  →  c ≈ 14.43        →  mu delta ≈ 2.41 (~9% less)
        // Bound > 2.5 fails the buggy formula; < 3.0 is a sanity cap.
        let mu_delta = ratings[0].mu - 25.0;
        assert!(
            mu_delta > 2.5,
            "winner mu delta {mu_delta:.4} too small — likely extra beta^2*n in c formula"
        );
        assert!(
            mu_delta < 3.0,
            "winner mu delta {mu_delta:.4} unexpectedly large"
        );
    }
}
