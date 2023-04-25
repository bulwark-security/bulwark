use crate::ThresholdError;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Outcome {
    Trusted,
    Accepted,
    Suspected,
    Restricted,
}

pub trait MassFunction
where
    Self: std::marker::Sized,
{
    fn new(accept: f64, restrict: f64, unknown: f64) -> Self;
    fn accept(&self) -> f64;
    fn restrict(&self) -> f64;
    fn unknown(&self) -> f64;

    // Pignistic reassigns unknown mass evenly to accept and restrict.
    fn pignistic(&self) -> Self {
        Self::new(
            self.accept() + self.unknown() / 2.0,
            self.restrict() + self.unknown() / 2.0,
            0.0,
        )
    }

    fn accepted(&self, threshold: f64) -> bool {
        let p = self.pignistic();
        p.accept() >= threshold
    }

    fn outcome(
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
        match p.restrict() {
            x if x <= trust => Ok(Outcome::Trusted),
            x if x < suspicious => Ok(Outcome::Accepted),
            x if x >= restrict => Ok(Outcome::Restricted),
            // x >= suspicious && x < restrict
            _ => Ok(Outcome::Suspected),
        }
    }

    fn clamp(&self) -> Self {
        self.clamp_min_unknown(0.0)
    }

    fn clamp_min_unknown(&self, min: f64) -> Self {
        let mut accept: f64 = self.accept();
        let mut restrict: f64 = self.restrict();
        let mut unknown: f64 = self.unknown();

        if accept < 0.0 {
            accept = 0.0
        } else if accept > 1.0 {
            accept = 1.0
        }
        if restrict < 0.0 {
            restrict = 0.0
        } else if restrict > 1.0 {
            restrict = 1.0
        }
        if unknown < 0.0 {
            unknown = 0.0
        } else if unknown > 1.0 {
            unknown = 1.0
        }
        if unknown < min {
            unknown = min
        }

        Self::new(accept, restrict, unknown)
    }

    fn fill_unknown(&self) -> Self {
        let sum = self.accept() + self.restrict() + self.unknown();
        Self::new(
            self.accept(),
            self.restrict(),
            if sum < 1.0 {
                1.0 - self.accept() - self.restrict()
            } else {
                self.unknown()
            },
        )
    }

    /// Rescales a [`MassFunction`] to ensure all component values are in the 0.0-1.0 range and sum to 1.0.
    ///
    /// It will preserve the relative relationship between [`accept`](MassFunction::accept) and
    /// [`restrict`](MassFunction::restrict).
    fn scale(&self) -> Self {
        self.scale_min_unknown(0.0)
    }

    /// Rescales a [`MassFunction`] to ensure all component values are in the 0.0-1.0 range and sum to 1.0 while
    /// ensuring that the [`unknown`](MassFunction::unknown) value is at least `min`.
    ///
    /// It will preserve the relative relationship between [`accept`](MassFunction::accept) and
    /// [`restrict`](MassFunction::restrict).
    fn scale_min_unknown(&self, min: f64) -> Self {
        let d = self.fill_unknown().clamp();
        let mut sum = d.accept() + d.restrict() + d.unknown();
        let mut accept = d.accept();
        let mut restrict = d.restrict();
        let mut unknown = d.unknown();

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
        Self::new(accept, restrict, unknown)
    }

    /// Multiplies the [`accept`](MassFunction::accept) and [`restrict`](MassFunction::restrict) by the `factor`
    /// parameter, replacing the [`unknown`](MassFunction::unknown) value with the remainder.
    ///
    /// Weights below 1.0 will reduce the weight of a [`MassFunction`], while weights above 1.0 will increase it.
    /// A 1.0 weight has no effect on the result, aside from scaling it to a valid range if necessary.
    fn weight(&self, factor: f64) -> Self {
        Self::new(self.accept() * factor, self.restrict() * factor, 0.0).scale()
    }

    /// Performs the conjunctive combination of two decisions.
    ///
    /// It is a helper function for [`combine`](MassFunction::combine).
    fn pairwise_combine(left: &Self, right: &Self) -> Self {
        // The mass assigned to the null hypothesis due to non-intersection.
        let nullh = left.accept() * right.restrict() + left.restrict() * right.accept();

        Self::new(
            // These are essentially an unrolled loop over the power set.
            // Each focal element from the left is multiplied by each on the right
            // and then appended to the intersection.
            // Finally, each focal element is normalized with respect to whatever
            // was assigned to the null hypothesis.
            (left.accept() * right.accept()
                + left.accept() * right.unknown()
                + left.unknown() * right.accept())
                / (1.0 - nullh),
            (left.restrict() * right.restrict()
                + left.restrict() * right.unknown()
                + left.unknown() * right.restrict())
                / (1.0 - nullh),
            (left.unknown() * right.unknown()) / (1.0 - nullh),
        )
    }

    fn find_min<'a, I>(vals: I) -> Option<&'a u32>
    where
        I: IntoIterator<Item = &'a u32>,
    {
        vals.into_iter().min()
    }

    /// Calculates the Murphy average of a set of decisions, returning a new [`MassFunction`] as the result.
    ///
    /// The Murphy average rule takes the mean value of each focal element across
    /// all mass functions to create a new mass function. This new mass function
    /// is then combined conjunctively with itself N times where N is the total
    /// number of functions that were averaged together.
    fn combine<'a, I>(decisions: I) -> Self
    where
        Self: 'a,
        I: IntoIterator<Item = &'a Self>,
    {
        let mut sum_a = 0.0;
        let mut sum_d = 0.0;
        let mut sum_u = 0.0;
        let mut length: usize = 0;
        for m in decisions {
            sum_a += m.accept();
            sum_d += m.restrict();
            sum_u += m.unknown();
            length += 1;
        }
        let avg_d = Self::new(
            sum_a / length as f64,
            sum_d / length as f64,
            sum_u / length as f64,
        );
        let mut d = Self::new(0.0, 0.0, 1.0);
        for _ in 0..length {
            d = Self::pairwise_combine(&d, &avg_d);
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub struct Decision {
        pub accept: f64,
        pub restrict: f64,
        pub unknown: f64,
    }

    impl MassFunction for Decision {
        fn new(accept: f64, restrict: f64, unknown: f64) -> Self {
            Decision {
                accept,
                restrict,
                unknown,
            }
        }

        fn accept(&self) -> f64 {
            self.accept
        }

        fn restrict(&self) -> f64 {
            self.restrict
        }

        fn unknown(&self) -> f64 {
            self.unknown
        }
    }

    macro_rules! test_decision {
        ($name:ident, $dec:expr $(, $attr:ident = $val:expr)*) => {
            #[test]
            fn $name() {
                $(assert_relative_eq!($dec.$attr, $val, epsilon = 2.0 * f64::EPSILON);)*
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
        accept = 0.5,
        restrict = 0.5,
        unknown = 0.0
    );

    #[test]
    fn decision_accepted() {
        let d = Decision {
            accept: 0.25,
            restrict: 0.2,
            unknown: 0.55,
        };
        assert!(d.accepted(0.5));

        let d = Decision {
            accept: 0.25,
            restrict: 0.25,
            unknown: 0.50,
        };
        assert!(d.accepted(0.5));

        let d = Decision {
            accept: 0.2,
            restrict: 0.25,
            unknown: 0.55,
        };
        assert!(!d.accepted(0.5));
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

    test_decision!(
        clamp_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .clamp(),
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
        accept = 1.0,
        restrict = 1.0,
        unknown = 1.0
    );

    test_decision!(
        scale_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .scale(),
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
        accept = 0.3333333333333333,
        restrict = 0.3333333333333333,
        unknown = 0.3333333333333333
    );

    test_decision!(
        weight_zero_by_zero,
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 0.0,
        }
        .weight(0.0),
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
        .weight(0.0),
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
        accept = 0.25,
        restrict = 0.25,
        unknown = 0.50
    );

    test_decision!(
        pairwise_combine_simple,
        MassFunction::pairwise_combine(
            &Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            &Decision {
                accept: 0.25,
                restrict: 0.1,
                unknown: 0.65,
            }
        ),
        accept = 0.338235294117647,
        restrict = 0.4705882352941177,
        unknown = 0.1911764705882353
    );

    test_decision!(
        pairwise_combine_factored_out,
        MassFunction::pairwise_combine(
            &Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            &Decision {
                accept: 0.0,
                restrict: 0.0,
                unknown: 1.0,
            }
        ),
        accept = 0.25,
        restrict = 0.5,
        unknown = 0.25
    );

    test_decision!(
        pairwise_combine_certainty,
        MassFunction::pairwise_combine(
            &Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            &Decision {
                accept: 1.0,
                restrict: 0.0,
                unknown: 0.0,
            }
        ),
        accept = 1.0,
        restrict = 0.0,
        unknown = 0.0
    );

    test_decision!(
        combine_simple_with_unknown,
        MassFunction::combine(&[
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
        accept = 0.2946891191709844,
        restrict = 0.16062176165803108,
        unknown = 0.5446891191709845
    );

    test_decision!(
        combine_high_conflict,
        MassFunction::combine(&[
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
        accept = 0.5,
        restrict = 0.5,
        unknown = 0.0
    );
}
