use crate::display::Segment;

pub const LOADING: [(usize, Segment); 12] = [
    (1, Segment::TopHorizontal),
    (2, Segment::TopHorizontal),
    (3, Segment::TopHorizontal),
    (4, Segment::TopHorizontal),
    (4, Segment::TopRightVertical),
    (4, Segment::BottomRightVertical),
    (4, Segment::BottomHorizontal),
    (3, Segment::BottomHorizontal),
    (2, Segment::BottomHorizontal),
    (1, Segment::BottomHorizontal),
    (1, Segment::BottomLeftVertical),
    (1, Segment::TopLeftVertical),
];
