//! # Sensor Election
//!
//! This crate is a placeholder for leader election logic. It will be used to
//! determine which sensor node is the leader of a group.

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
