//! Use case: learn the feature weights from the user.
//!
//! A committed composition the user accepted, or one they corrected, gives a
//! gold path.  When the model's best path differs, a perceptron step moves
//! the weights towards the gold path:
//!
//!   w_k += lr_k · (f_k(predicted) − f_k(gold))
//!
//! (costs: a *lower* cost wins, so features the gold path has more of get
//! cheaper).  Cost-type weights are scale factors and move slowly in a narrow
//! band; indicator weights move in Mozc units and are bounded.  Nothing is
//! learned when the prediction already equals the gold path.

use crate::application::weights::{lower_bound, Kind, Weights, FEATURE_KINDS, N_FEATURES};

#[derive(Debug, Clone)]
pub struct Learner {
    /// step per unit of feature difference, indicator features (Mozc units)
    pub lr_indicator: f32,
    /// step per 1000 units of cost-feature difference
    pub lr_cost: f32,
    pub indicator_bound: f32,
    pub cost_min: f32,
    pub cost_max: f32,
    /// number of updates performed
    pub updates: u64,
}

impl Default for Learner {
    fn default() -> Self {
        Learner { lr_indicator: 300.0, lr_cost: 0.02, indicator_bound: 8000.0, cost_min: 0.0, cost_max: 3.0, updates: 0 }
    }
}

impl Learner {
    /// Apply one perceptron step.  Returns true when weights changed.
    pub fn update(&mut self, w: &mut Weights, predicted: &[f32], gold: &[f32]) -> bool {
        if predicted.len() != N_FEATURES || gold.len() != N_FEATURES {
            return false;
        }
        let mut changed = false;
        for k in 0..N_FEATURES {
            let d = predicted[k] - gold[k];
            if d == 0.0 {
                continue;
            }
            match FEATURE_KINDS[k] {
                Kind::Indicator => {
                    let lowest = lower_bound(k).max(-self.indicator_bound);
                    w.w[k] = (w.w[k] + self.lr_indicator * d.signum() * d.abs().min(3.0)).clamp(lowest, self.indicator_bound);
                }
                Kind::Cost => {
                    w.w[k] = (w.w[k] + self.lr_cost * (d / 1000.0).clamp(-3.0, 3.0)).clamp(self.cost_min, self.cost_max);
                }
            }
            changed = true;
        }
        if changed {
            self.updates += 1;
        }
        changed
    }
}
