use crate::MassFunction;
use validator::{Validate, ValidationError};

// While tag vectors are closely related to the Decision type, by not
// including them as fields and handling them separately, we allow
// the Decision type to be Copy-able and we avoid unnecessary cloning
// that would otherwise need to be done when operating on a Decision.

// TODO: implement From traits for converting back and forth from Decisions and DecisionInterface types

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
            }
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
            }
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
            }
        ),
        true,
        accept = 1.0,
        restrict = 0.0,
        unknown = 0.0
    );

    test_decision!(
        combine_simple_with_unknown,
        Decision::combine(&[
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
        Decision::combine(&[
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
