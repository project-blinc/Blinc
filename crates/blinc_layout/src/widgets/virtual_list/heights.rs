//! Measured row heights with logarithmic prefix sums and offset lookup.
#[derive(Clone)]
pub(super) struct Heights {
    estimate: f32,
    values: Vec<f32>,
    sums: Vec<f32>,
}
impl Heights {
    pub fn new(count: usize, estimate: f32) -> Self {
        let mut this = Self {
            estimate,
            values: vec![estimate; count],
            sums: vec![0.0; count + 1],
        };
        for i in 0..count {
            this.add(i, estimate);
        }
        this
    }
    pub fn reconfigure(&mut self, count: usize, estimate: f32) -> bool {
        if self.values.len() == count && self.estimate == estimate {
            return false;
        }
        if self.estimate != estimate {
            *self = Self::new(count, estimate);
        } else {
            self.values.resize(count, estimate);
            self.sums = vec![0.0; count + 1];
            for i in 0..count {
                self.add(i, self.values[i]);
            }
        }
        true
    }
    fn add(&mut self, index: usize, delta: f32) {
        let mut i = index + 1;
        while i < self.sums.len() {
            self.sums[i] += delta;
            i += i & i.wrapping_neg();
        }
    }
    pub fn measure(&mut self, index: usize, height: f32) -> bool {
        if !height.is_finite()
            || height <= 0.0
            || index >= self.values.len()
            || (self.values[index] - height).abs() < 0.5
        {
            return false;
        }
        let delta = height - self.values[index];
        self.values[index] = height;
        self.add(index, delta);
        true
    }
    pub fn prefix(&self, count: usize) -> f32 {
        let mut i = count.min(self.values.len());
        let mut sum = 0.0;
        while i > 0 {
            sum += self.sums[i];
            i &= i - 1;
        }
        sum
    }
    pub fn index_at(&self, offset: f32) -> usize {
        let mut index = 0;
        let mut sum = 0.0;
        let mut step = self.sums.len().next_power_of_two() / 2;
        while step > 0 {
            let next = index + step;
            if next < self.sums.len() && sum + self.sums[next] <= offset.max(0.0) {
                index = next;
                sum += self.sums[next];
            }
            step /= 2;
        }
        index.min(self.values.len().saturating_sub(1))
    }
    pub fn window(&self, offset: f32, viewport: f32, budget: usize) -> (usize, usize, f32, f32) {
        let count = self.values.len();
        if count == 0 {
            return (0, 0, 0.0, 0.0);
        }
        let first = self.index_at(offset);
        let last = (self.index_at(offset + viewport.max(1.0)) + 1).min(count);
        let spare = budget.saturating_sub(last - first).max(4);
        let start = first.saturating_sub(spare / 2);
        let end = (last + spare / 2).min(count);
        (
            start,
            end,
            self.prefix(start),
            (self.prefix(count) - self.prefix(end)).max(0.0),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_rows_update_prefixes_and_scroll_windows() {
        let mut heights = Heights::new(10_000, 40.0);
        assert_eq!(heights.index_at(1000.0), 25);
        assert!(heights.measure(0, 80.0));
        assert_eq!(heights.prefix(2), 120.0);
        assert_eq!(heights.index_at(1000.0), 24);
        let (start, end, top, bottom) = heights.window(399_800.0, 216.0, 24);
        assert!(start > 9900);
        assert_eq!(end, 10_000);
        assert_eq!(bottom, 0.0);
        assert_eq!(top, heights.prefix(start));
        assert_eq!(
            Heights::new(0, 40.0).window(200.0, 100.0, 24),
            (0, 0, 0.0, 0.0)
        );
    }
}
