use validator::{Validate, ValidationError};

#[derive(Debug, Validate, Copy, Clone)]
#[validate(schema(function = "validate_sum", skip_on_field_errors = false))]
pub struct Decision {
    #[validate(range(min = 0.0, max = 1.0))]
    pub allow: f64,
    #[validate(range(min = 0.0, max = 1.0))]
    pub deny: f64,
    #[validate(range(min = 0.0, max = 1.0))]
    pub unknown: f64
}

fn validate_sum(decision: &Decision) -> Result<(), ValidationError> {
    let sum = decision.allow + decision.deny + decision.unknown;
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
    // Pignistic reassigns unknown mass evenly to allow and deny.
    pub fn pignistic(&self) -> Decision {
        return Decision{
            allow: self.allow + self.unknown / 2.0,
            deny: self.deny + self.unknown / 2.0,
            unknown: 0.0
        }
    }

    pub fn allowed(&self) -> bool {
        let p = self.pignistic();
        return p.allow >= p.deny
    }

    pub fn clamp(&self) -> Decision {
        return self.clamp_min_unknown(0.0)
    }

    pub fn clamp_min_unknown(&self, min: f64) -> Decision {
        let mut allow: f64 = self.allow;
        let mut deny: f64 = self.deny;
        let mut unknown: f64 = self.unknown;

        if allow < 0.0 {
            allow = 0.0
        } else if allow > 1.0 {
            allow = 1.0
        }
        if deny < 0.0 {
            deny = 0.0
        } else if deny > 1.0 {
            deny = 1.0
        }
        if unknown < 0.0 {
            unknown = 0.0
        } else if unknown > 1.0 {
            unknown = 1.0
        }
        if unknown < min {
            unknown = min
        }
    
        return Decision{
            allow: allow,
            deny: deny,
            unknown: unknown
        }
    }

    pub fn fill_unknown(&self) -> Decision {
        let sum = self.allow + self.deny + self.unknown;
        return Decision{
            allow: self.allow,
            deny: self.deny,
            unknown: if sum < 1.0 {
                1.0 - self.allow - self.deny
            } else {
                self.unknown
            }
        }
    }

    pub fn scale(&self) -> Decision {
        return self.scale_min_unknown(0.0)
    }

    pub fn scale_min_unknown(&self, min: f64) -> Decision {
        let mut d = self.fill_unknown().clamp();
        let mut sum = d.allow + d.deny + d.unknown;
    
        if sum > 0.0 {
            d.allow = d.allow / sum;
            d.deny = d.deny / sum;
            d.unknown = d.unknown / sum
        }
        if d.unknown < min {
            d.unknown = min
        }
        sum = 1.0 - d.unknown;
        if sum > 0.0 {
            let denominator = d.allow + d.deny;
            d.allow = sum * (d.allow / denominator);
            d.deny = sum * (d.deny / denominator)
        }
        return d
    }

    pub fn weight(&self, factor: f64) -> Decision {
        return Decision{
            allow: self.allow * factor,
            deny: self.deny * factor,
            unknown: 0.0
        }.scale()
    }
}

// pairwise_combine performs the conjunctive combination of two decisions
// helper function for combine
fn pairwise_combine(left: Decision, right: Decision) -> Decision {
    // The mass assigned to the null hypothesis due to non-intersection.
    let nullh = left.allow * right.deny + left.deny * right.allow;
    let d = Decision{
        // These are essentially an unrolled loop over the power set.
        // Each focal element from the left is multiplied by each on the right
        // and then appended to the intersection.
        // Finally, each focal element is normalized with respect to whatever
        // was assigned to the null hypothesis.
        allow: (
            left.allow * right.allow +
            left.allow * right.unknown +
            left.unknown * right.allow
        ) / (1.0 - nullh),
        deny: (
            left.deny * right.deny +
            left.deny * right.unknown +
            left.unknown * right.deny
        ) / (1.0 - nullh),
        unknown: (left.unknown * right.unknown) / (1.0 - nullh)
    };
    return d
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
        sum_a += m.allow;
        sum_d += m.deny;
        sum_u += m.unknown
    }
    let avg_d = Decision{
        allow: sum_a / decisions.len() as f64,
        deny: sum_d / decisions.len() as f64,
        unknown: sum_u / decisions.len() as f64
    };
    let mut d = Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 1.0
    };
    for _ in 0..decisions.len() {
        d = pairwise_combine(d, avg_d)
    }
    return d
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

    test_decision!(validate_zero, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 0.0
    }, false, allow = 0.0, deny = 0.0, unknown = 0.0);

    test_decision!(validate_simple, Decision{
        allow: 0.25,
        deny: 0.25,
        unknown: 0.50
    }, true, allow = 0.25, deny = 0.25, unknown = 0.50);

    test_decision!(validate_negative, Decision{
        allow: -0.25,
        deny: 0.75,
        unknown: 0.50
    }, false, allow = -0.25, deny = 0.75, unknown = 0.50);

    test_decision!(pignistic_simple, Decision{
        allow: 0.25,
        deny: 0.25,
        unknown: 0.50
    }.pignistic(), true, allow = 0.5, deny = 0.5, unknown = 0.0);

    #[test]
    fn decision_allowed() {
        let d = Decision{
            allow: 0.25,
            deny: 0.2,
            unknown: 0.55
        };
        assert!(d.allowed());

        let d = Decision{
            allow: 0.25,
            deny: 0.25,
            unknown: 0.50
        };
        assert!(d.allowed());

        let d = Decision{
            allow: 0.2,
            deny: 0.25,
            unknown: 0.55
        };
        assert!(!d.allowed());
    }

    test_decision!(clamp_zero, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 0.0
    }.clamp(), false, allow = 0.0, deny = 0.0, unknown = 0.0);

    test_decision!(clamp_three_halves, Decision{
        allow: 0.50,
        deny: 0.50,
        unknown: 0.50
    }.clamp(), false, allow = 0.50, deny = 0.50, unknown = 0.50);

    test_decision!(clamp_three_whole, Decision{
        allow: 1.0,
        deny: 1.0,
        unknown: 1.0
    }.clamp(), false, allow = 1.0, deny = 1.0, unknown = 1.0);

    test_decision!(clamp_negative, Decision{
        allow: -1.0,
        deny: -1.0,
        unknown: -1.0
    }.clamp(), false, allow = 0.0, deny = 0.0, unknown = 0.0);

    test_decision!(clamp_triple_double, Decision{
        allow: 2.0,
        deny: 2.0,
        unknown: 2.0
    }.clamp(), false, allow = 1.0, deny = 1.0, unknown = 1.0);

    test_decision!(scale_zero, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 0.0
    }.scale(), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(scale_unknown, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 1.0
    }.scale(), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(scale_negative, Decision{
        allow: -1.0,
        deny: -1.0,
        unknown: -1.0
    }.scale(), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(scale_double, Decision{
        allow: 2.0,
        deny: 2.0,
        unknown: 2.0
    }.scale(), true,
    allow = 0.3333333333333333,
    deny = 0.3333333333333333,
    unknown = 0.3333333333333333);

    test_decision!(weight_zero_by_zero, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 0.0
    }.weight(0.0), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(weight_zero_by_one, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 0.0
    }.weight(0.0), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(weight_one_by_zero, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 1.0
    }.weight(0.0), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(weight_one_by_one, Decision{
        allow: 0.0,
        deny: 0.0,
        unknown: 1.0
    }.weight(1.0), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(weight_negative, Decision{
        allow: -1.0,
        deny: -1.0,
        unknown: -1.0
    }.weight(1.0), true, allow = 0.0, deny = 0.0, unknown = 1.0);

    test_decision!(weight_two_by_one, Decision{
        allow: 2.0,
        deny: 2.0,
        unknown: 2.0
    }.weight(1.0), true, allow = 0.50, deny = 0.50, unknown = 0.0);

    test_decision!(weight_two_by_one_eighth, Decision{
        allow: 2.0,
        deny: 2.0,
        unknown: 2.0
    }.weight(0.125), true, allow = 0.25, deny = 0.25, unknown = 0.5);

    test_decision!(weight_one_eighth_by_two, Decision{
        allow: 0.125,
        deny: 0.125,
        unknown: 0.75
    }.weight(2.0), true, allow = 0.25, deny = 0.25, unknown = 0.50);

    test_decision!(pairwise_combine_simple, pairwise_combine(
        Decision{
            allow: 0.25,
            deny: 0.5,
            unknown: 0.25
        },
        Decision{
            allow: 0.25,
            deny: 0.1,
            unknown: 0.65
        }
    ), true, allow = 0.338235294117647, deny = 0.4705882352941177, unknown = 0.1911764705882353);

    test_decision!(pairwise_combine_factored_out, pairwise_combine(
        Decision{
            allow: 0.25,
            deny: 0.5,
            unknown: 0.25
        },
        Decision{
            allow: 0.0,
            deny: 0.0,
            unknown: 1.0
        }
    ), true, allow = 0.25, deny = 0.5, unknown = 0.25);

    test_decision!(pairwise_combine_certainty, pairwise_combine(
        Decision{
            allow: 0.25,
            deny: 0.5,
            unknown: 0.25
        },
        Decision{
            allow: 1.0,
            deny: 0.0,
            unknown: 0.0
        }
    ), true, allow = 1.0, deny = 0.0, unknown = 0.0);

    test_decision!(combine_simple_with_unknown, combine(&[
        Decision{
            allow: 0.35,
            deny: 0.20,
            unknown: 0.45
        },
        Decision{
            allow: 0.0,
            deny: 0.0,
            unknown: 1.0
        }
    ]), true, allow = 0.2946891191709844, deny = 0.16062176165803108, unknown = 0.5446891191709845);

    test_decision!(combine_high_conflict, combine(&[
        Decision{
            allow: 1.0,
            deny: 0.0,
            unknown: 0.0
        },
        Decision{
            allow: 0.0,
            deny: 1.0,
            unknown: 0.0
        }
    ]), true, allow = 0.5, deny = 0.5, unknown = 0.0);
}
