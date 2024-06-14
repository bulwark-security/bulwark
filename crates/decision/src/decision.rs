use crate::ThresholdError;
use strum_macros::{Display, EnumString};
use validator::{Validate, ValidationError};

// While tag vectors are closely related to the Decision type, by not
// including them as fields and handling them separately, we allow
// the Decision type to be Copy-able and we avoid unnecessary cloning
// that would otherwise need to be done when operating on a Decision.
// It also potentially allows for Decision to be used in other contexts.

/// Represents a value from a continuous range taken from the [`pignistic`](Decision::pignistic)
/// transformation as a category that can be used to select a response to an operation.
#[derive(PartialEq, Eq, Clone, Copy, Debug, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Outcome {
    Trusted,
    Accepted,
    Suspected,
    Restricted,
}

/// A `Decision` represents evidence in favor of either accepting or restricting an operation under consideration.
///
/// It is composed of three values: `accept`, `restrict` and `unknown`. Each must be between 0.0 and 1.0 inclusive
/// and the sum of all three must equal 1.0. The `unknown` value represents uncertainty about the evidence, with
/// a 1.0 `unknown` value indicating total uncertainty or a "no opinion" verdict. Similarly, a 1.0 `accept` or
/// `restrict` value indicates total certainty that the verdict should be to accept or to restrict, respectively.
///
/// This representation allows for a fairly intuitive way of characterizing evidence in favor of or against
/// blocking an operation, while still capturing any uncertainty. Limiting to two states rather than a wider range of
/// classification possibilities allows for better performance optimizations, simplifies code readability, and
/// enables useful transformations like reweighting a `Decision`.
///
/// This data structure is a two-state [Dempster-Shafer](https://en.wikipedia.org/wiki/Dempster%E2%80%93Shafer_theory)
/// mass function, with the power set represented by the `unknown` value. This enables the use of combination rules
/// to aggregate decisions from multiple sources. However, knowledge of Dempster-Shafer theory should not be necessary.
#[derive(Debug, Validate, Copy, Clone, PartialEq)]
#[validate(schema(function = "validate_sum", skip_on_field_errors = false))]
pub struct Decision {
    #[validate(range(min = 0.0, max = 1.0))]
    pub accept: f64,
    #[validate(range(min = 0.0, max = 1.0))]
    pub restrict: f64,
    #[validate(range(min = 0.0, max = 1.0))]
    pub unknown: f64,
}

impl Default for Decision {
    /// The default [`Decision`] assigns nothing to the `accept` and `restrict` components and everything
    /// to the `unknown` component.
    fn default() -> Self {
        UNKNOWN
    }
}

/// A decision representing acceptance with full certainty.
pub const ACCEPT: Decision = Decision {
    accept: 1.0,
    restrict: 0.0,
    unknown: 0.0,
};

/// A decision representing restriction with full certainty.
pub const RESTRICT: Decision = Decision {
    accept: 0.0,
    restrict: 1.0,
    unknown: 0.0,
};

/// A decision representing full uncertainty.
pub const UNKNOWN: Decision = Decision {
    accept: 0.0,
    restrict: 0.0,
    unknown: 1.0,
};

/// Validates that a `Decision`'s components correctly sum to 1.0.
fn validate_sum(decision: &Decision) -> Result<(), ValidationError> {
    let sum = decision.accept + decision.restrict + decision.unknown;
    if sum < 0.0 - 2.0 * f64::EPSILON {
        return Err(ValidationError::new("sum cannot be negative"));
    } else if sum > 1.0 + 2.0 * f64::EPSILON {
        return Err(ValidationError::new("sum cannot be greater than one"));
    } else if !(sum > 1.0 - 2.0 * f64::EPSILON && sum < 1.0 + 2.0 * f64::EPSILON) {
        return Err(ValidationError::new("sum should be equal to one"));
    }

    Ok(())
}

impl Decision {
    /// Converts a simple scalar value into a `Decision` using the value as the `accept` component.
    ///
    /// This function is sugar for `Decision { accept, 0.0, 0.0 }.scale())`.
    ///
    /// # Arguments
    ///
    /// * `accept` - The `accept` value to set.
    ///
    /// # Examples
    ///
    /// ```
    /// use approx::assert_relative_eq;
    /// use bulwark_decision::Decision;
    ///
    /// assert_relative_eq!(Decision::accepted(1.0), Decision { accept: 1.0, restrict: 0.0, unknown: 0.0 });
    /// assert_relative_eq!(Decision::accepted(0.5), Decision { accept: 0.5, restrict: 0.0, unknown: 0.5 });
    /// assert_relative_eq!(Decision::accepted(0.0), Decision { accept: 0.0, restrict: 0.0, unknown: 1.0 });
    /// ```
    pub fn accepted(accept: f64) -> Self {
        Self {
            accept,
            restrict: 0.0,
            unknown: 0.0,
        }
        .scale()
    }

    /// Converts a simple scalar value into a `Decision` using the value as the `restrict` component.
    ///
    /// This function is sugar for `Decision { 0.0, restrict, 0.0 }.scale())`.
    ///
    /// # Arguments
    ///
    /// * `restrict` - The `restrict` value to set.
    ///
    /// # Examples
    ///
    /// ```
    /// use approx::assert_relative_eq;
    /// use bulwark_decision::Decision;
    ///
    /// assert_relative_eq!(Decision::restricted(1.0), Decision { accept: 0.0, restrict: 1.0, unknown: 0.0 });
    /// assert_relative_eq!(Decision::restricted(0.5), Decision { accept: 0.0, restrict: 0.5, unknown: 0.5 });
    /// assert_relative_eq!(Decision::restricted(0.0), Decision { accept: 0.0, restrict: 0.0, unknown: 1.0 });
    /// ```
    pub fn restricted(restrict: f64) -> Self {
        Self {
            accept: 0.0,
            restrict,
            unknown: 0.0,
        }
        .scale()
    }

    /// Reassigns unknown mass evenly to accept and restrict.
    ///
    /// This function is used to convert to a form that is useful in producing a final outcome.
    pub fn pignistic(&self) -> Self {
        Self {
            accept: self.accept + self.unknown / 2.0,
            restrict: self.restrict + self.unknown / 2.0,
            unknown: 0.0,
        }
    }

    /// Checks the [`accept`](Decision::accept) value after [`pignistic`](Decision::pignistic)
    /// transformation against a threshold value. `true` if above the threshold.
    ///
    /// # Arguments
    ///
    /// * `threshold` - The minimum value required to accept a [`Decision`].
    pub fn is_accepted(&self, threshold: f64) -> bool {
        let p = self.pignistic();
        p.accept >= threshold
    }

    /// Checks that the [`unknown`](Decision::unknown) value is non-zero while
    /// [`accept`](Decision::accept) and [`restrict`](Decision::restrict) are both zero.
    pub fn is_unknown(&self) -> bool {
        self.unknown >= f64::EPSILON
            && self.accept >= 0.0
            && self.accept <= f64::EPSILON
            && self.restrict >= 0.0
            && self.restrict <= f64::EPSILON
    }

    /// Checks the [`restrict`](Decision::restrict) value after [`pignistic`](Decision::pignistic)
    /// transformation against several threshold values.
    ///
    /// The [`Outcome`]s are arranged in ascending order: `Trusted` < `Accepted` < `Suspected` < `Restricted`
    ///
    /// Does not take an `accept` threshold to simplify validation. Returns [`ThresholdError`] if threshold values are
    /// either out-of-order or out-of-range. Thresholds must be between 0.0 and 1.0.
    ///
    /// # Arguments
    ///
    /// * `trust` - The `trust` threshold is an upper-bound threshold. If the `restrict` value is below it, the
    ///     operation is `Trusted`.
    /// * `suspicious` - The `suspicious` threshold is a lower-bound threshold that also defines the accepted range.
    ///     If the `restrict` value is above the `trust` threshold and below the `suspicious` threshold, the operation
    ///     is `Accepted`. If the `restrict` value is above the `suspicious` threshold but below the `restrict`
    ///     threshold, the operation is `Suspected`.
    /// * `restrict` -  The `restricted` threshold is a lower-bound threshold. If the `restrict` value is above it,
    ///     the operation is `Restricted`.
    pub fn outcome(
        &self,
        trust: f64,
        suspicious: f64,
        restrict: f64,
    ) -> Result<Outcome, ThresholdError> {
        let p = self.pignistic();
        if trust > suspicious || suspicious > restrict {
            return Err(ThresholdError::ThresholdOutOfOrder);
        }
        if !(0.0..=1.0).contains(&trust) {
            return Err(ThresholdError::ThresholdOutOfRange(trust));
        }
        if !(0.0..=1.0).contains(&suspicious) {
            return Err(ThresholdError::ThresholdOutOfRange(suspicious));
        }
        if !(0.0..=1.0).contains(&restrict) {
            return Err(ThresholdError::ThresholdOutOfRange(restrict));
        }
        match p.restrict {
            x if x <= trust => Ok(Outcome::Trusted),
            x if x < suspicious => Ok(Outcome::Accepted),
            x if x >= restrict => Ok(Outcome::Restricted),
            // x >= suspicious && x < restrict
            _ => Ok(Outcome::Suspected),
        }
    }

    /// Clamps all values to the 0.0 to 1.0 range.
    ///
    /// Does not guarantee that values will sum to 1.0.
    pub fn clamp(&self) -> Self {
        self.clamp_min_unknown(0.0)
    }

    /// Clamps all values to the 0.0 to 1.0 range, guaranteeing that the unknown value will be at least `min`.
    ///
    /// Does not guarantee that values will sum to 1.0.
    ///
    /// # Arguments
    ///
    /// * `min` - The minimum [`unknown`](Decision::unknown) value.
    pub fn clamp_min_unknown(&self, min: f64) -> Self {
        let accept: f64 = self.accept.clamp(0.0, 1.0);
        let restrict: f64 = self.restrict.clamp(0.0, 1.0);
        let unknown: f64 = self.unknown.clamp(min.max(0.0), 1.0);

        Self {
            accept,
            restrict,
            unknown,
        }
    }

    /// If the component values sum to less than 1.0, assigns the remainder to the
    /// [`unknown`](Decision::unknown) value.
    pub fn fill_unknown(&self) -> Self {
        // Check for negative unknown values, much of this function's behavior becomes unintuitive otherwise.
        let unknown = if self.unknown + f64::EPSILON >= 0.0 {
            self.unknown
        } else {
            0.0
        };
        let sum = self.accept + self.restrict + unknown;
        Self {
            accept: self.accept,
            restrict: self.restrict,
            unknown: if sum - f64::EPSILON <= 1.0 {
                1.0 - self.accept - self.restrict
            } else {
                unknown
            },
        }
    }

    /// Rescales a [`Decision`] to ensure all component values are in the 0.0-1.0 range and sum to 1.0.
    ///
    /// It will preserve the relative relationship between [`accept`](Decision::accept) and
    /// [`restrict`](Decision::restrict).
    pub fn scale(&self) -> Self {
        self.scale_min_unknown(0.0)
    }

    /// Rescales a [`Decision`] to ensure all component values are in the 0.0-1.0 range and sum to 1.0 while
    /// ensuring that the [`unknown`](Decision::unknown) value is at least `min`.
    ///
    /// It will preserve the relative relationship between [`accept`](Decision::accept) and
    /// [`restrict`](Decision::restrict).
    ///
    /// # Arguments
    ///
    /// * `min` - The minimum [`unknown`](Decision::unknown) value.
    pub fn scale_min_unknown(&self, min: f64) -> Self {
        let d = self.fill_unknown().clamp();
        let mut sum = d.accept + d.restrict + d.unknown;
        let mut accept = d.accept;
        let mut restrict = d.restrict;
        let mut unknown = d.unknown;

        if sum > 0.0 {
            accept /= sum;
            restrict /= sum;
            unknown /= sum
        }
        if unknown < min {
            unknown = min
        }
        sum = 1.0 - unknown;
        if sum > 0.0 {
            let denominator = accept + restrict;
            accept = sum * (accept / denominator);
            restrict = sum * (restrict / denominator)
        }
        Self {
            accept,
            restrict,
            unknown,
        }
    }

    /// Multiplies the [`accept`](Decision::accept) and [`restrict`](Decision::restrict) by the `factor`
    /// parameter, replacing the [`unknown`](Decision::unknown) value with the remainder.
    ///
    /// Weights below 1.0 will reduce the weight of a [`Decision`], while weights above 1.0 will increase it.
    /// A 1.0 weight has no effect on the result, aside from scaling it to a valid range if necessary.
    ///
    /// # Arguments
    ///
    /// * `factor` - A scale factor used to multiply the [`accept`](Decision::accept) and
    ///     [`restrict`](Decision::restrict) values.
    pub fn weight(&self, factor: f64) -> Self {
        Self {
            accept: self.accept * factor,
            restrict: self.restrict * factor,
            unknown: 0.0,
        }
        .scale()
    }

    /// Performs the conjunctive combination of two decisions.
    ///
    /// It is a helper function for [`combine`](Decision::combine).
    ///
    /// # Arguments
    ///
    /// * `left` - The first [`Decision`] of the pair.
    /// * `right` - The second [`Decision`] of the pair.
    fn pairwise_combine(left: &Self, right: &Self, normalize: bool) -> Self {
        // The mass assigned to the null hypothesis due to non-intersection.
        let nullh = if normalize {
            left.accept * right.restrict + left.restrict * right.accept
        } else {
            // If normalization is disabled, just ignore the null hypothesis.
            0.0
        };

        Self {
            // These are essentially an unrolled loop over the power set.
            // Each focal element from the left is multiplied by each on the right
            // and then appended to the intersection.
            // Finally, each focal element is normalized with respect to whatever
            // was assigned to the null hypothesis.
            accept: (left.accept * right.accept
                + left.accept * right.unknown
                + left.unknown * right.accept)
                / (1.0 - nullh),
            restrict: (left.restrict * right.restrict
                + left.restrict * right.unknown
                + left.unknown * right.restrict)
                / (1.0 - nullh),
            unknown: (left.unknown * right.unknown) / (1.0 - nullh),
        }
    }

    /// Calculates the conjunctive combination of a set of decisions, returning a new [`Decision`] as the result.
    ///
    /// Unlike [`combine_murphy`](Decision::combine_murphy), `combine_conjunctive` will produce a `NaN` result under
    /// high conflict.
    ///
    /// # Arguments
    ///
    /// * `decisions` - The `Decision`s to be combined.
    pub fn combine_conjunctive<'a, I>(decisions: I) -> Self
    where
        Self: 'a,
        I: IntoIterator<Item = &'a Self>,
    {
        let mut d = Self {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        };
        for m in decisions {
            d = Self::pairwise_combine(&d, m, true);
        }
        d
    }

    /// Calculates the Murphy average of a set of decisions, returning a new [`Decision`] as the result.
    ///
    /// The Murphy average rule[^1] takes the mean value of each focal element across
    /// all mass functions to create a new mass function. This new mass function
    /// is then combined conjunctively with itself N times where N is the total
    /// number of functions that were averaged together.
    ///
    /// # Arguments
    ///
    /// * `decisions` - The `Decision`s to be combined.
    ///
    /// [^1]: Catherine K. Murphy. 2000. Combining belief functions when evidence conflicts.
    ///     Decision Support Systems 29, 1 (2000), 1-9. DOI:<https://doi.org/10.1016/s0167-9236(99)00084-6>
    pub fn combine_murphy<'a, I>(decisions: I) -> Self
    where
        Self: 'a,
        I: IntoIterator<Item = &'a Self>,
    {
        let mut sum_a = 0.0;
        let mut sum_d = 0.0;
        let mut sum_u = 0.0;
        let mut length: usize = 0;
        for m in decisions {
            sum_a += m.accept;
            sum_d += m.restrict;
            sum_u += m.unknown;
            length += 1;
        }
        let avg_d = Self {
            accept: sum_a / length as f64,
            restrict: sum_d / length as f64,
            unknown: sum_u / length as f64,
        };
        let mut d = Self {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        };
        for _ in 0..length {
            d = Self::pairwise_combine(&d, &avg_d, true);
        }
        d
    }

    /// Calculates the degree of conflict between a set of Decisions.
    ///
    /// # Arguments
    ///
    /// * `decisions` - The `Decision`s to measure conflict for.
    pub fn conflict<'a, I>(decisions: I) -> f64
    where
        Self: 'a,
        I: IntoIterator<Item = &'a Self>,
    {
        let mut d = Self {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        };
        for m in decisions {
            d = Self::pairwise_combine(&d, m, false);
        }
        let diff = d.accept + d.restrict + d.unknown;
        if diff > 0.0 {
            -diff.ln()
        } else {
            f64::INFINITY
        }
    }
}

impl approx::AbsDiffEq for Decision {
    type Epsilon = <f64 as approx::AbsDiffEq>::Epsilon;

    fn default_epsilon() -> Self::Epsilon {
        <f64 as approx::AbsDiffEq>::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        f64::abs_diff_eq(&self.accept, &other.accept, epsilon)
            && f64::abs_diff_eq(&self.restrict, &other.restrict, epsilon)
            && f64::abs_diff_eq(&self.unknown, &other.unknown, epsilon)
    }
}

impl approx::RelativeEq for Decision {
    fn default_max_relative() -> Self::Epsilon {
        <f64 as approx::RelativeEq>::default_max_relative()
    }

    fn relative_eq(
        &self,
        other: &Self,
        epsilon: Self::Epsilon,
        max_relative: Self::Epsilon,
    ) -> bool {
        f64::relative_eq(&self.accept, &other.accept, epsilon, max_relative)
            && f64::relative_eq(&self.restrict, &other.restrict, epsilon, max_relative)
            && f64::relative_eq(&self.unknown, &other.unknown, epsilon, max_relative)
    }
}

impl approx::UlpsEq for Decision {
    fn default_max_ulps() -> u32 {
        <f64 as approx::UlpsEq>::default_max_ulps()
    }

    fn ulps_eq(&self, other: &Self, epsilon: Self::Epsilon, max_ulps: u32) -> bool {
        f64::ulps_eq(&self.accept, &other.accept, epsilon, max_ulps)
            && f64::ulps_eq(&self.restrict, &other.restrict, epsilon, max_ulps)
            && f64::ulps_eq(&self.unknown, &other.unknown, epsilon, max_ulps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_decision {
        ($name:ident, $dec:expr, $v:expr $(, $attr:ident = $val:expr)*) => {
            #[test]
            fn $name() {
                $(assert_relative_eq!($dec.$attr, $val, epsilon = 2.0 * f64::EPSILON);)*
                // test suite repeated, this time with validation
                if $v {
                    match $dec.validate() {
                        Ok(_) => assert!(true),
                        Err(e) => assert!(false, "decision should have validated: {}", e),
                    }
                } else {
                    match $dec.validate() {
                        Ok(_) => assert!(false, "decision should not validate"),
                        Err(_) => assert!(true),
                    }
                }
            }
        }
    }

    test_decision!(
        validate_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        },
        false,
        accept = 0.0,
        restrict = 0.0,
        unknown = 0.0
    );

    test_decision!(
        validate_simple,
        Decision {
            accept: 0.25,
            restrict: 0.25,
            unknown: 0.50,
        },
        true,
        accept = 0.25,
        restrict = 0.25,
        unknown = 0.50
    );

    test_decision!(
        validate_negative,
        Decision {
            accept: -0.25,
            restrict: 0.75,
            unknown: 0.50,
        },
        false,
        accept = -0.25,
        restrict = 0.75,
        unknown = 0.50
    );

    test_decision!(
        pignistic_simple,
        Decision {
            accept: 0.25,
            restrict: 0.25,
            unknown: 0.50,
        }
        .pignistic(),
        true,
        accept = 0.5,
        restrict = 0.5,
        unknown = 0.0
    );

    test_decision!(
        clamp_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .clamp(),
        false,
        accept = 0.0,
        restrict = 0.0,
        unknown = 0.0
    );

    test_decision!(
        clamp_three_halves,
        Decision {
            accept: 0.50,
            restrict: 0.50,
            unknown: 0.50,
        }
        .clamp(),
        false,
        accept = 0.50,
        restrict = 0.50,
        unknown = 0.50
    );

    test_decision!(
        clamp_three_whole,
        Decision {
            accept: 1.0,
            restrict: 1.0,
            unknown: 1.0,
        }
        .clamp(),
        false,
        accept = 1.0,
        restrict = 1.0,
        unknown = 1.0
    );

    test_decision!(
        clamp_negative,
        Decision {
            accept: -1.0,
            restrict: -1.0,
            unknown: -1.0,
        }
        .clamp(),
        false,
        accept = 0.0,
        restrict = 0.0,
        unknown = 0.0
    );

    test_decision!(
        clamp_triple_double,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: 2.0,
        }
        .clamp(),
        false,
        accept = 1.0,
        restrict = 1.0,
        unknown = 1.0
    );

    test_decision!(
        clamp_min_unknown,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: -1.0,
        }
        .clamp_min_unknown(0.3),
        false,
        accept = 1.0,
        restrict = 1.0,
        unknown = 0.3
    );

    test_decision!(
        fill_unknown_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .fill_unknown(),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        fill_unknown_normal,
        Decision {
            accept: 0.25,
            restrict: 0.25,
            unknown: 0.1,
        }
        .fill_unknown(),
        true,
        accept = 0.25,
        restrict = 0.25,
        unknown = 0.5
    );

    test_decision!(
        fill_unknown_over_one,
        Decision {
            accept: 0.5,
            restrict: 0.5,
            unknown: 0.5,
        }
        .fill_unknown(),
        false,
        accept = 0.5,
        restrict = 0.5,
        unknown = 0.5
    );

    test_decision!(
        fill_unknown_mixed,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: -3.5,
        }
        .fill_unknown(),
        false,
        accept = 2.0,
        restrict = 2.0,
        unknown = 0.0
    );

    test_decision!(
        scale_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .scale(),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        scale_unknown,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        }
        .scale(),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        scale_negative,
        Decision {
            accept: -1.0,
            restrict: -1.0,
            unknown: -1.0,
        }
        .scale(),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        scale_double,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: 2.0,
        }
        .scale(),
        true,
        accept = 0.3333333333333333,
        restrict = 0.3333333333333333,
        unknown = 0.3333333333333333
    );

    test_decision!(
        scale_mixed,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: -3.5,
        }
        .scale(),
        true,
        accept = 0.5,
        restrict = 0.5,
        unknown = 0.0
    );

    test_decision!(
        weight_zero_by_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .weight(0.0),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        weight_zero_by_one,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .weight(1.0),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        weight_one_by_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        }
        .weight(0.0),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        weight_one_by_one,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        }
        .weight(1.0),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        weight_negative,
        Decision {
            accept: -1.0,
            restrict: -1.0,
            unknown: -1.0,
        }
        .weight(1.0),
        true,
        accept = 0.0,
        restrict = 0.0,
        unknown = 1.0
    );

    test_decision!(
        weight_two_by_one,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: 2.0,
        }
        .weight(1.0),
        true,
        accept = 0.50,
        restrict = 0.50,
        unknown = 0.0
    );

    test_decision!(
        weight_two_by_two,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: 2.0,
        }
        .weight(2.0),
        true,
        accept = 0.50,
        restrict = 0.50,
        unknown = 0.0
    );

    test_decision!(
        weight_two_by_one_eighth,
        Decision {
            accept: 2.0,
            restrict: 2.0,
            unknown: 2.0,
        }
        .weight(0.125),
        true,
        accept = 0.25,
        restrict = 0.25,
        unknown = 0.5
    );

    test_decision!(
        weight_one_eighth_by_two,
        Decision {
            accept: 0.125,
            restrict: 0.125,
            unknown: 0.75,
        }
        .weight(2.0),
        true,
        accept = 0.25,
        restrict = 0.25,
        unknown = 0.50
    );

    test_decision!(
        pairwise_combine_simple,
        Decision::pairwise_combine(
            &Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            &Decision {
                accept: 0.25,
                restrict: 0.1,
                unknown: 0.65,
            },
            true,
        ),
        true,
        accept = 0.338235294117647,
        restrict = 0.4705882352941177,
        unknown = 0.1911764705882353
    );

    test_decision!(
        pairwise_combine_factored_out,
        Decision::pairwise_combine(
            &Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            &Decision {
                accept: 0.0,
                restrict: 0.0,
                unknown: 1.0,
            },
            true,
        ),
        true,
        accept = 0.25,
        restrict = 0.5,
        unknown = 0.25
    );

    test_decision!(
        pairwise_combine_certainty,
        Decision::pairwise_combine(
            &Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            &Decision {
                accept: 1.0,
                restrict: 0.0,
                unknown: 0.0,
            },
            true,
        ),
        true,
        accept = 1.0,
        restrict = 0.0,
        unknown = 0.0
    );

    test_decision!(
        combine_conjunctive_simple_with_unknown,
        Decision::combine_conjunctive(&[
            Decision {
                accept: 0.35,
                restrict: 0.20,
                unknown: 0.45,
            },
            Decision {
                accept: 0.0,
                restrict: 0.0,
                unknown: 1.0,
            }
        ]),
        true,
        accept = 0.35,
        restrict = 0.2,
        unknown = 0.45
    );

    test_decision!(
        combine_murphy_simple_with_unknown,
        Decision::combine_murphy(&[
            Decision {
                accept: 0.35,
                restrict: 0.20,
                unknown: 0.45,
            },
            Decision {
                accept: 0.0,
                restrict: 0.0,
                unknown: 1.0,
            }
        ]),
        true,
        accept = 0.2946891191709844,
        restrict = 0.16062176165803108,
        unknown = 0.5446891191709845
    );

    #[test]
    fn test_combine_conjunctive_high_conflict() {
        let d = Decision::combine_conjunctive(&[
            Decision {
                accept: 1.0,
                restrict: 0.0,
                unknown: 0.0,
            },
            Decision {
                accept: 0.0,
                restrict: 1.0,
                unknown: 0.0,
            },
        ]);
        assert!(d.accept.is_nan());
        assert!(d.restrict.is_nan());
        assert!(d.unknown.is_nan());
    }

    test_decision!(
        combine_murphy_high_conflict,
        Decision::combine_murphy(&[
            Decision {
                accept: 1.0,
                restrict: 0.0,
                unknown: 0.0,
            },
            Decision {
                accept: 0.0,
                restrict: 1.0,
                unknown: 0.0,
            }
        ]),
        true,
        accept = 0.5,
        restrict = 0.5,
        unknown = 0.0
    );

    #[test]
    fn decision_is_accepted() {
        let d = Decision {
            accept: 0.25,
            restrict: 0.2,
            unknown: 0.55,
        };
        assert!(d.is_accepted(0.5));

        let d = Decision {
            accept: 0.25,
            restrict: 0.25,
            unknown: 0.50,
        };
        assert!(d.is_accepted(0.5));

        let d = Decision {
            accept: 0.2,
            restrict: 0.25,
            unknown: 0.55,
        };
        assert!(!d.is_accepted(0.5));
    }

    #[test]
    fn decision_is_unknown() {
        let d = Decision {
            accept: 0.0,
            restrict: 0.2,
            unknown: 0.8,
        };
        assert!(!d.is_unknown());

        let d = Decision {
            accept: 0.2,
            restrict: 0.0,
            unknown: 0.8,
        };
        assert!(!d.is_unknown());

        let d = Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        };
        assert!(d.is_unknown());

        let d = Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.1,
        };
        assert!(d.is_unknown());
    }

    #[test]
    fn decision_outcome() -> Result<(), Box<dyn std::error::Error>> {
        let d = Decision {
            accept: 0.65,
            restrict: 0.0,
            unknown: 0.35,
        };
        let outcome = d.outcome(0.2, 0.4, 0.8)?;
        assert_eq!(outcome, Outcome::Trusted);

        let d = Decision {
            accept: 0.45,
            restrict: 0.05,
            unknown: 0.5,
        };
        let outcome = d.outcome(0.2, 0.4, 0.8)?;
        assert_eq!(outcome, Outcome::Accepted);

        let d = Decision {
            accept: 0.25,
            restrict: 0.2,
            unknown: 0.55,
        };
        let outcome = d.outcome(0.2, 0.4, 0.8)?;
        assert_eq!(outcome, Outcome::Suspected);

        let d = Decision {
            accept: 0.05,
            restrict: 0.65,
            unknown: 0.3,
        };
        let outcome = d.outcome(0.2, 0.4, 0.8)?;
        assert_eq!(outcome, Outcome::Restricted);

        Ok(())
    }

    #[test]
    fn decision_conflict() -> Result<(), Box<dyn std::error::Error>> {
        // Maximum possible conflict
        let decisions = &[
            Decision {
                accept: 1.0,
                restrict: 0.0,
                unknown: 0.0,
            },
            Decision {
                accept: 0.0,
                restrict: 1.0,
                unknown: 0.0,
            },
        ];
        let conflict = Decision::conflict(decisions);
        assert_eq!(conflict, f64::INFINITY);

        // Perfect agreement
        let decisions = &[
            Decision {
                accept: 1.0,
                restrict: 0.0,
                unknown: 0.0,
            },
            Decision {
                accept: 0.25,
                restrict: 0.0,
                unknown: 0.75,
            },
            Decision {
                accept: 0.5,
                restrict: 0.0,
                unknown: 0.5,
            },
            Decision {
                accept: 0.75,
                restrict: 0.0,
                unknown: 0.25,
            },
            Decision {
                accept: 0.0,
                restrict: 0.0,
                unknown: 1.0,
            },
        ];
        let conflict = Decision::conflict(decisions);
        assert_relative_eq!(conflict, 0.0, epsilon = 2.0 * f64::EPSILON);

        // Simple two-decision conflict
        let decisions = &[
            Decision {
                accept: 0.25,
                restrict: 0.0,
                unknown: 0.75,
            },
            Decision {
                accept: 0.0,
                restrict: 0.5,
                unknown: 0.5,
            },
        ];
        let conflict = Decision::conflict(decisions);
        // null hypothesis = 0.125, conflict = -ln(1.0 - nullh)
        assert_relative_eq!(conflict, 0.13353139262452263, epsilon = 2.0 * f64::EPSILON);

        // Complex multi-way conflict
        let decisions = &[
            Decision {
                accept: 0.25,
                restrict: 0.25,
                unknown: 0.50,
            },
            Decision {
                accept: 0.25,
                restrict: 0.0,
                unknown: 0.75,
            },
            Decision {
                accept: 0.0,
                restrict: 0.5,
                unknown: 0.5,
            },
            Decision {
                accept: 0.35,
                restrict: 0.20,
                unknown: 0.45,
            },
        ];
        let conflict = Decision::conflict(decisions);
        // null hypothesis = 0.41875, conflict = -ln(1.0 - nullh)
        assert_relative_eq!(conflict, 0.5425743220805709, epsilon = 2.0 * f64::EPSILON);

        // Internal conflict
        let decisions = &[
            Decision {
                accept: 0.35,
                restrict: 0.20,
                unknown: 0.45,
            },
            Decision {
                accept: 0.35,
                restrict: 0.20,
                unknown: 0.45,
            },
            Decision {
                accept: 0.35,
                restrict: 0.20,
                unknown: 0.45,
            },
            Decision {
                accept: 0.35,
                restrict: 0.20,
                unknown: 0.45,
            },
        ];
        let conflict = Decision::conflict(decisions);
        // null hypothesis = 0.4529, conflict = -ln(1.0 - nullh)
        assert_relative_eq!(conflict, 0.6031236779123568, epsilon = 2.0 * f64::EPSILON);

        Ok(())
    }
}
