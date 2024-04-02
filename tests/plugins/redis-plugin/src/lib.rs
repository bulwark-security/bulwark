use bulwark_wasm_sdk::*;
use std::collections::HashMap;

pub struct RedisPlugin;

#[bulwark_plugin]
impl HttpHandlers for RedisPlugin {
    fn handle_init() -> Result<(), Error> {
        // Cleans up in case a prior test failed and left things in a bad state.
        redis::del([
            "test:redis-set-get",
            "test:redis-incr-get",
            "test:redis-sadd-srem-smembers",
            "bulwark:rl:test:redis-rate-limit",
            "bulwark:rl:test:redis-rate-limit:exp",
            "bulwark:bk:g:test:redis-circuit-breaker",
            "bulwark:bk:s:test:redis-circuit-breaker",
            "bulwark:bk:f:test:redis-circuit-breaker",
            "bulwark:bk:cs:test:redis-circuit-breaker",
            "bulwark:bk:cf:test:redis-circuit-breaker",
        ])?;

        Ok(())
    }

    fn handle_request_decision(
        _request: Request,
        _labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        let mut output = HandlerOutput::default();

        // Test key/value and increment operations.
        assert_eq!(redis::get("test:does-not-exist")?, None);
        assert_eq!(
            redis::del(["test:does-not-exist", "test:also-does-not-exist"])?,
            0
        );
        redis::set("test:redis-set-get", "some value")?;
        assert_eq!(
            redis::get("test:redis-set-get")?,
            Some("some value".as_bytes().to_vec())
        );
        assert_eq!(redis::del(["test:redis-set-get"])?, 1);
        redis::incr("test:redis-incr-get")?;
        assert_eq!(
            redis::get("test:redis-incr-get")?,
            Some("1".as_bytes().to_vec())
        );
        redis::incr("test:redis-incr-get")?;
        assert_eq!(
            redis::get("test:redis-incr-get")?,
            Some("2".as_bytes().to_vec())
        );
        redis::incr_by("test:redis-incr-get", 5)?;
        assert_eq!(
            redis::get("test:redis-incr-get")?,
            Some("7".as_bytes().to_vec())
        );

        // Test set operations.
        redis::sadd("test:redis-sadd-srem-smembers", ["apple"])?;
        redis::sadd("test:redis-sadd-srem-smembers", ["apple"])?;
        redis::sadd("test:redis-sadd-srem-smembers", ["orange"])?;
        redis::sadd("test:redis-sadd-srem-smembers", ["pear"])?;
        assert_eq!(redis::srem("test:redis-sadd-srem-smembers", ["apple"])?, 1);
        let members = redis::smembers("test:redis-sadd-srem-smembers")?;
        assert!(members.contains(&"orange".to_string()));
        assert!(members.contains(&"pear".to_string()));
        assert_eq!(members.len(), 2);
        assert_eq!(redis::srem("test:does-not-exist", ["apple"])?, 0);

        // Test rate limit operations.
        let rate = redis::check_rate_limit("test:does-not-exist")?;
        assert!(rate.is_none());
        let rate = redis::incr_rate_limit("test:redis-rate-limit", 1, 10)?;
        let original_expiration = rate.expiration;
        assert_eq!(rate.attempts, 1);
        let rate = redis::incr_rate_limit("test:redis-rate-limit", 1, 10)?;
        assert_eq!(rate.attempts, 2);
        assert_eq!(rate.expiration, original_expiration);
        let rate = redis::incr_rate_limit("test:redis-rate-limit", 5, 10)?;
        assert_eq!(rate.attempts, 7);
        assert_eq!(rate.expiration, original_expiration);
        let rate = redis::check_rate_limit("test:redis-rate-limit")?;
        assert!(rate.is_some());
        if let Some(rate) = rate {
            assert_eq!(rate.attempts, 7);
            assert_eq!(rate.expiration, original_expiration);
        }

        // Test circuit breaker operations.
        let breaker = redis::check_breaker("test:does-not-exist")?;
        assert!(breaker.is_none());
        let breaker = redis::incr_breaker("test:redis-circuit-breaker", 1, true, 10)?;
        let original_expiration = breaker.expiration;
        assert_eq!(breaker.generation, 1);
        assert_eq!(breaker.successes, 1);
        assert_eq!(breaker.failures, 0);
        assert_eq!(breaker.consecutive_successes, 1);
        assert_eq!(breaker.consecutive_failures, 0);
        let breaker = redis::incr_breaker("test:redis-circuit-breaker", 1, true, 10)?;
        assert_eq!(breaker.generation, 2);
        assert_eq!(breaker.successes, 2);
        assert_eq!(breaker.failures, 0);
        assert_eq!(breaker.consecutive_successes, 2);
        assert_eq!(breaker.consecutive_failures, 0);
        assert_eq!(breaker.expiration, original_expiration);
        let breaker = redis::incr_breaker("test:redis-circuit-breaker", 1, false, 10)?;
        assert_eq!(breaker.generation, 3);
        assert_eq!(breaker.successes, 2);
        assert_eq!(breaker.failures, 1);
        assert_eq!(breaker.consecutive_successes, 0);
        assert_eq!(breaker.consecutive_failures, 1);
        assert_eq!(breaker.expiration, original_expiration);
        let breaker = redis::incr_breaker("test:redis-circuit-breaker", 3, false, 10)?;
        assert_eq!(breaker.generation, 4);
        assert_eq!(breaker.successes, 2);
        assert_eq!(breaker.failures, 4);
        assert_eq!(breaker.consecutive_successes, 0);
        assert_eq!(breaker.consecutive_failures, 4);
        assert_eq!(breaker.expiration, original_expiration);
        let breaker = redis::incr_breaker("test:redis-circuit-breaker", 2, true, 10)?;
        assert_eq!(breaker.generation, 5);
        assert_eq!(breaker.successes, 4);
        assert_eq!(breaker.failures, 4);
        assert_eq!(breaker.consecutive_successes, 2);
        assert_eq!(breaker.consecutive_failures, 0);
        assert_eq!(breaker.expiration, original_expiration);
        let breaker = redis::check_breaker("test:redis-circuit-breaker")?;
        assert!(breaker.is_some());
        if let Some(breaker) = breaker {
            assert_eq!(breaker.generation, 5);
            assert_eq!(breaker.successes, 4);
            assert_eq!(breaker.failures, 4);
            assert_eq!(breaker.consecutive_successes, 2);
            assert_eq!(breaker.consecutive_failures, 0);
            assert_eq!(breaker.expiration, original_expiration);
        }

        // Test expiration operations.
        redis::expire("test:does-not-exist", 1)?;
        redis::expire("test:redis-incr-get", 1)?;
        redis::expire_at("test:does-not-exist", 1)?;
        redis::expire_at("test:redis-sadd-srem-smembers", 1)?;
        // Only verify that there's no error.

        // Teardown.
        redis::del([
            "test:redis-set-get",
            "test:redis-incr-get",
            "test:redis-sadd-srem-smembers",
            "bulwark:rl:test:redis-rate-limit",
            "bulwark:rl:test:redis-rate-limit:exp",
            "bulwark:bk:g:test:redis-circuit-breaker",
            "bulwark:bk:s:test:redis-circuit-breaker",
            "bulwark:bk:f:test:redis-circuit-breaker",
            "bulwark:bk:cs:test:redis-circuit-breaker",
            "bulwark:bk:cf:test:redis-circuit-breaker",
        ])?;

        output.decision = Decision::restricted(0.0);
        Ok(output)
    }
}
