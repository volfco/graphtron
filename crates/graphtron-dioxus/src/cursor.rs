/// Shared cursor state for one chart or a synced group of charts.
///
/// The `GraphtronChart` component reads and writes a `Signal<Cursor>`. Pass the
/// same signal to several charts to sync their crosshairs (the obs console
/// pattern); pass nothing and each chart creates its own local cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    /// Live hover position (epoch ms), tracking the pointer.
    pub hover: Option<i64>,
    /// Frozen crosshair position (epoch ms), set by click.
    pub frozen: Option<i64>,
}

impl Cursor {
    pub fn toggle_freeze(&mut self, ts: i64) {
        self.frozen = if self.frozen.is_some() {
            None
        } else {
            Some(ts)
        };
    }
}
