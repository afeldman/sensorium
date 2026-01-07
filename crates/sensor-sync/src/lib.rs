//! # Sensor Sync
//!
//! This crate is a placeholder for time synchronization logic. It will be used to
//! synchronize the clocks of the sensor nodes.

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
