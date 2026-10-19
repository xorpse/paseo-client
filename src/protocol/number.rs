use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(transparent)]
pub(crate) struct WireNumber(f64);

impl WireNumber {
    pub(crate) fn whole_u32(self) -> Option<u32> {
        self.is_whole_within(0.0, f64::from(u32::MAX))
            .then_some(self.0 as u32)
    }

    pub(crate) fn whole_i32(self) -> Option<i32> {
        self.is_whole_within(f64::from(i32::MIN), f64::from(i32::MAX))
            .then_some(self.0 as i32)
    }

    pub(crate) fn count(self) -> u64 {
        self.0 as u64
    }

    fn is_whole_within(self, min: f64, max: f64) -> bool {
        self.0.fract() == 0.0 && (min..=max).contains(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::WireNumber;

    #[test]
    fn whole_numbers_convert_and_others_do_not() {
        assert_eq!(WireNumber(42.0).whole_u32(), Some(42));
        assert_eq!(WireNumber(-1.0).whole_u32(), None);
        assert_eq!(WireNumber(2.5).whole_u32(), None);
        assert_eq!(WireNumber(4_294_967_296.0).whole_u32(), None);
        assert_eq!(WireNumber(-3.0).whole_i32(), Some(-3));
        assert_eq!(WireNumber(0.5).whole_i32(), None);
    }

    #[test]
    fn counts_saturate() {
        assert_eq!(WireNumber(7.0).count(), 7);
        assert_eq!(WireNumber(7.9).count(), 7);
        assert_eq!(WireNumber(-4.0).count(), 0);
        assert_eq!(WireNumber(1e30).count(), u64::MAX);
    }
}
