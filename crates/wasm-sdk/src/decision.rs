use validator::{Validate, ValidationError};

#[derive(Debug, Validate, Copy, Clone)]
#[validate(schema(function = "validate_sum", skip_on_field_errors = false))]
pub struct Decision {
    #[validate(range(min = 0.0, max = 1.0))]
    pub accept: f64,
    #[validate(range(min = 0.0, max = 1.0))]
    pub restrict: f64,
    #[validate(range(min = 0.0, max = 1.0))]
    pub unknown: f64,
}

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
    // Pignistic reassigns unknown mass evenly to accept and restrict.
    pub fn pignistic(&self) -> Decision {
        Decision {
            accept: self.accept + self.unknown / 2.0,
            restrict: self.restrict + self.unknown / 2.0,
            unknown: 0.0,
        }
    }

    pub fn accepted(&self) -> bool {
        let p = self.pignistic();
        p.accept >= p.restrict
    }

    pub fn clamp(&self) -> Decision {
        self.clamp_min_unknown(0.0)
    }

    pub fn clamp_min_unknown(&self, min: f64) -> Decision {
        let mut accept: f64 = self.accept;
        let mut restrict: f64 = self.restrict;
        let mut unknown: f64 = self.unknown;

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

        Decision {
            accept,
            restrict,
            unknown,
        }
    }

    pub fn fill_unknown(&self) -> Decision {
        let sum = self.accept + self.restrict + self.unknown;
        Decision {
            accept: self.accept,
            restrict: self.restrict,
            unknown: if sum < 1.0 {
                1.0 - self.accept - self.restrict
            } else {
                self.unknown
            },
        }
    }

    pub fn scale(&self) -> Decision {
        self.scale_min_unknown(0.0)
    }

    pub fn scale_min_unknown(&self, min: f64) -> Decision {
        let mut d = self.fill_unknown().clamp();
        let mut sum = d.accept + d.restrict + d.unknown;

        if sum > 0.0 {
            d.accept /= sum;
            d.restrict /= sum;
            d.unknown /= sum
        }
        if d.unknown < min {
            d.unknown = min
        }
        sum = 1.0 - d.unknown;
        if sum > 0.0 {
            let denominator = d.accept + d.restrict;
            d.accept = sum * (d.accept / denominator);
            d.restrict = sum * (d.restrict / denominator)
        }
        d
    }

    pub fn weight(&self, factor: f64) -> Decision {
        Decision {
            accept: self.accept * factor,
            restrict: self.restrict * factor,
            unknown: 0.0,
        }
        .scale()
    }
}

// pairwise_combine performs the conjunctive combination of two decisions
// helper function for combine
fn pairwise_combine(left: Decision, right: Decision) -> Decision {
    // The mass assigned to the null hypothesis due to non-intersection.
    let nullh = left.accept * right.restrict + left.restrict * right.accept;

    Decision {
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

// combine takes the Murphy average of a set of decisions,
// returning a new decision as the result.
//
// The Murphy average rule takes the mean value of each focal element across
// all mass functions to create a new mass function. This new mass function
// is then combined conjunctively with itself N times where N is the total
// number of functions that were averaged together.
pub fn combine(decisions: &[Decision]) -> Decision {
    let mut sum_a = 0.0;
    let mut sum_d = 0.0;
    let mut sum_u = 0.0;
    for m in decisions {
        sum_a += m.accept;
        sum_d += m.restrict;
        sum_u += m.unknown;
    }
    let avg_d = Decision {
        accept: sum_a / decisions.len() as f64,
        restrict: sum_d / decisions.len() as f64,
        unknown: sum_u / decisions.len() as f64,
    };
    let mut d = Decision {
        accept: 0.0,
        restrict: 0.0,
        unknown: 1.0,
    };
    for _ in 0..decisions.len() {
        d = pairwise_combine(d, avg_d)
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_decision {
        ($name:ident, $dec:expr, $v:expr $(, $attr:ident = $val:expr)*) => {
            #[test]
            fn $name() {
                $(assert_relative_eq!($dec.$attr, $val, epsilon = 2.0 * f64::EPSILON);)*
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

    #[test]
    fn decision_accepted() {
        let d = Decision {
            accept: 0.25,
            restrict: 0.2,
            unknown: 0.55,
        };
        assert!(d.accepted());

        let d = Decision {
            accept: 0.25,
            restrict: 0.25,
            unknown: 0.50,
        };
        assert!(d.accepted());

        let d = Decision {
            accept: 0.2,
            restrict: 0.25,
            unknown: 0.55,
        };
        assert!(!d.accepted());
    }

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
        .weight(0.0),
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
        pairwise_combine(
            Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            Decision {
                accept: 0.25,
                restrict: 0.1,
                unknown: 0.65,
            }
        ),
        true,
        accept = 0.338235294117647,
        restrict = 0.4705882352941177,
        unknown = 0.1911764705882353
    );

    test_decision!(
        pairwise_combine_factored_out,
        pairwise_combine(
            Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            Decision {
                accept: 0.0,
                restrict: 0.0,
                unknown: 1.0,
            }
        ),
        true,
        accept = 0.25,
        restrict = 0.5,
        unknown = 0.25
    );

    test_decision!(
        pairwise_combine_certainty,
        pairwise_combine(
            Decision {
                accept: 0.25,
                restrict: 0.5,
                unknown: 0.25,
            },
            Decision {
                accept: 1.0,
                restrict: 0.0,
                unknown: 0.0,
            }
        ),
        true,
        accept = 1.0,
        restrict = 0.0,
        unknown = 0.0
    );

    test_decision!(
        combine_simple_with_unknown,
        combine(&[
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

    test_decision!(
        combine_high_conflict,
        combine(&[
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
}
