use std::time::Duration;

/// Maps capture time onto active recording time, cutting paused intervals out of every track.
#[derive(Default)]
pub struct Timeline {
    pauses: Vec<(Duration, Option<Duration>)>,
}
impl Timeline {
    pub fn paused(&self) -> bool {
        self.pauses.last().is_some_and(|p| p.1.is_none())
    }
    pub fn pause(&mut self, at: Duration) {
        if !self.paused() {
            self.pauses.push((at, None));
        }
    }
    pub fn resume(&mut self, at: Duration) {
        if let Some((_, end @ None)) = self.pauses.last_mut() {
            *end = Some(at);
        }
    }
    pub fn map(&self, at: Duration) -> Option<Duration> {
        let mut removed = Duration::ZERO;
        for &(start, end) in &self.pauses {
            if at < start {
                break;
            }
            match end {
                Some(end) if at >= end => removed += end - start,
                _ => return None,
            }
        }
        Some(at.saturating_sub(removed))
    }
    pub fn active(&self, at: Duration) -> Duration {
        let mut removed = Duration::ZERO;
        for &(start, end) in &self.pauses {
            if at >= start {
                removed += end.unwrap_or(at).min(at) - start;
            }
        }
        at.saturating_sub(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(n: u64) -> Duration {
        Duration::from_secs(n)
    }
    #[test]
    fn delayed_packets_are_mapped_by_capture_time_including_pause_boundaries() {
        let mut t = Timeline::default();
        t.pause(s(2));
        t.resume(s(5));
        t.pause(s(8));
        t.resume(s(9));
        assert_eq!(t.map(s(1)), Some(s(1)));
        assert_eq!(t.map(s(2)), None);
        assert_eq!(t.map(s(4)), None);
        assert_eq!(t.map(s(5)), Some(s(2)));
        assert_eq!(t.map(s(10)), Some(s(6)));
        t.pause(s(11));
        assert_eq!(t.active(s(14)), s(7));
        assert_eq!(t.map(s(12)), None);
    }
}
