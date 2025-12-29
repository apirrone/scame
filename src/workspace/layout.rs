use crate::workspace::BufferId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaneId {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    Single,         // Only left pane visible
    VerticalSplit,  // Both panes visible
}

pub struct LayoutManager {
    mode: LayoutMode,
    active_pane: PaneId,
    left_buffer: Option<BufferId>,
    right_buffer: Option<BufferId>,
    split_position: u16,  // Column where split occurs
}

impl LayoutManager {
    pub fn new() -> Self {
        Self {
            mode: LayoutMode::Single,
            active_pane: PaneId::Left,
            left_buffer: None,
            right_buffer: None,
            split_position: 0,
        }
    }

    pub fn mode(&self) -> LayoutMode {
        self.mode
    }

    pub fn active_pane(&self) -> PaneId {
        self.active_pane
    }

    pub fn left_buffer(&self) -> Option<BufferId> {
        self.left_buffer
    }

    pub fn right_buffer(&self) -> Option<BufferId> {
        self.right_buffer
    }

    pub fn toggle_split(&mut self, total_width: u16) {
        match self.mode {
            LayoutMode::Single => {
                self.mode = LayoutMode::VerticalSplit;
                self.split_position = total_width / 2;
            }
            LayoutMode::VerticalSplit => {
                self.mode = LayoutMode::Single;
            }
        }
    }

    pub fn close_split(&mut self) {
        self.mode = LayoutMode::Single;
        self.active_pane = PaneId::Left;
    }

    pub fn active_buffer(&self) -> Option<BufferId> {
        match self.active_pane {
            PaneId::Left => self.left_buffer,
            PaneId::Right => self.right_buffer,
        }
    }

    pub fn set_buffer(&mut self, pane: PaneId, buffer_id: BufferId) {
        match pane {
            PaneId::Left => self.left_buffer = Some(buffer_id),
            PaneId::Right => self.right_buffer = Some(buffer_id),
        }
    }

    pub fn switch_pane(&mut self) {
        if self.mode == LayoutMode::VerticalSplit {
            self.active_pane = match self.active_pane {
                PaneId::Left => PaneId::Right,
                PaneId::Right => PaneId::Left,
            };
        }
    }

    pub fn recalculate_split(&mut self, total_width: u16) {
        if self.mode == LayoutMode::VerticalSplit {
            self.split_position = total_width / 2;
        }
    }

    pub fn pane_dimensions(&self, term_width: u16, term_height: u16) -> PaneDimensions {
        let tab_bar_height = 1;
        let status_bar_height = 1;
        let content_height = term_height.saturating_sub(tab_bar_height + status_bar_height);

        match self.mode {
            LayoutMode::Single => PaneDimensions {
                left: PaneRect {
                    x: 0,
                    y: tab_bar_height,
                    width: term_width,
                    height: content_height,
                },
                right: None,
            },
            LayoutMode::VerticalSplit => {
                let left_width = self.split_position;
                let right_width = term_width.saturating_sub(left_width);
                PaneDimensions {
                    left: PaneRect {
                        x: 0,
                        y: tab_bar_height,
                        width: left_width,
                        height: content_height,
                    },
                    right: Some(PaneRect {
                        x: left_width,
                        y: tab_bar_height,
                        width: right_width,
                        height: content_height,
                    }),
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PaneRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

pub struct PaneDimensions {
    pub left: PaneRect,
    pub right: Option<PaneRect>,
}
