use crate::display::{self, CharacterIndex, DisplayError, Segment};

pub struct CharacterSegment {
    pub character: CharacterIndex,
    pub segment: Segment,
}

pub struct AnimationFrame {
    pub character_segments: &'static [CharacterSegment],
}

pub struct Animation {
    pub duration: u64,
    pub frames: &'static [AnimationFrame],
}

macro_rules! frame {
    () => {
        AnimationFrame {
            character_segments: &[],
        }
    };
    ($($char:expr => [$($segment:expr),+ $(,)?]),+ $(,)?) => {
        AnimationFrame {
            character_segments: &[
                $($(
                    CharacterSegment {
                        character: $char,
                        segment: $segment,
                    }
                ),+),+
            ],
        }
    };
}

const fn snow_anim(heavy: bool) -> Animation {
    if heavy {
        Animation {
            duration: 150,
            frames: &[
                frame!(
                    CharacterIndex::One => [Segment::TopMiddleVertical, Segment::MiddleLeft, Segment::MiddleRight, Segment::BottomMiddleVertical],
                    CharacterIndex::Two => [Segment::TopMiddleVertical, Segment::MiddleLeft, Segment::MiddleRight, Segment::BottomMiddleVertical],
                    CharacterIndex::Three => [Segment::TopMiddleVertical, Segment::MiddleLeft, Segment::MiddleRight, Segment::BottomMiddleVertical],
                    CharacterIndex::Four => [Segment::TopMiddleVertical, Segment::MiddleLeft, Segment::MiddleRight, Segment::BottomMiddleVertical],
                ),
                frame!(
                    CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::BottomLeftDiagonal, Segment::BottomRightDiagonal],
                    CharacterIndex::Two => [Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::BottomLeftDiagonal, Segment::BottomRightDiagonal],
                    CharacterIndex::Three => [Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::BottomLeftDiagonal, Segment::BottomRightDiagonal],
                    CharacterIndex::Four => [Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::BottomLeftDiagonal, Segment::BottomRightDiagonal],
                ),
            ],
        }
    } else {
        Animation {
            duration: 250,
            frames: &[
                frame!(
                    CharacterIndex::One => [Segment::TopMiddleVertical, Segment::MiddleLeft, Segment::MiddleRight, Segment::BottomMiddleVertical],
                    CharacterIndex::Three => [Segment::TopMiddleVertical, Segment::MiddleLeft, Segment::MiddleRight, Segment::BottomMiddleVertical],
                ),
                frame!(
                    CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::BottomLeftDiagonal, Segment::BottomRightDiagonal],
                    CharacterIndex::Three => [Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::BottomLeftDiagonal, Segment::BottomRightDiagonal],
                ),
            ],
        }
    }
}

pub const SNOW: Animation = snow_anim(false);
pub const HEAVY_SNOW: Animation = snow_anim(true);

pub const LOADING: Animation = Animation {
    duration: 500,
    frames: &[
        frame!(CharacterIndex::One => [Segment::TopHorizontal]),
        frame!(CharacterIndex::Two => [Segment::TopHorizontal]),
        frame!(CharacterIndex::Three => [Segment::TopHorizontal]),
        frame!(CharacterIndex::Four => [Segment::TopHorizontal]),
        frame!(CharacterIndex::Four => [Segment::TopRightVertical]),
        frame!(CharacterIndex::Four => [Segment::BottomRightVertical]),
        frame!(CharacterIndex::Four => [Segment::BottomHorizontal]),
        frame!(CharacterIndex::Three => [Segment::BottomHorizontal]),
        frame!(CharacterIndex::Two => [Segment::BottomHorizontal]),
        frame!(CharacterIndex::One => [Segment::BottomHorizontal]),
        frame!(CharacterIndex::One => [Segment::BottomLeftVertical]),
        frame!(CharacterIndex::One => [Segment::TopLeftVertical]),
    ],
};

const fn rain_anim(heavy: bool) -> Animation {
    Animation {
        duration: if heavy { 500 } else { 500 },
        frames: &[
            frame!(
                CharacterIndex::One => [Segment::TopLeftDiagonal],
                CharacterIndex::Four => [Segment::BottomRightDiagonal],
            ),
            frame!(
                CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal],
                CharacterIndex::Three => [Segment::TopLeftDiagonal],
            ),
            frame!(
                CharacterIndex::One => [Segment::BottomRightDiagonal],
                CharacterIndex::Three => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal],
            ),
            frame!(
                CharacterIndex::Two => [Segment::TopLeftDiagonal],
                CharacterIndex::Three => [Segment::BottomRightDiagonal],
            ),
            frame!(
                CharacterIndex::Two => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal],
                CharacterIndex::Four => [Segment::TopLeftDiagonal],
            ),
            frame!(
                CharacterIndex::Two => [Segment::BottomRightDiagonal],
                CharacterIndex::Four => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal],
            ),
        ],
    }
}

pub const RAIN: Animation = rain_anim(false);
pub const HEAVY_RAIN: Animation = rain_anim(true);

pub const SUNSHINE: Animation = Animation {
    duration: 750,
    frames: &[
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft],
            CharacterIndex::Two => [Segment::TopHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft],
            CharacterIndex::Two => [Segment::MiddleLeft],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft, Segment::BottomRightDiagonal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft, Segment::BottomLeftVertical],
        ),
    ],
};

pub const LIGHTNING: Animation = Animation {
    duration: 500,
    frames: &[
        frame!(CharacterIndex::One => [Segment::TopLeftDiagonal]),
        frame!(CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::MiddleRight]),
        frame!(
            CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::MiddleRight],
            CharacterIndex::Two => [Segment::MiddleLeft],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::MiddleRight],
            CharacterIndex::Two => [Segment::MiddleLeft, Segment::BottomRightDiagonal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::MiddleRight],
            CharacterIndex::Two => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomRightVertical, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::MiddleRight],
            CharacterIndex::Two => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomRightVertical, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::Two => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomRightVertical, Segment::BottomHorizontal],
        ),
        frame!(),
        frame!(),
    ],
};

pub const CLOUDY: Animation = Animation {
    duration: 1500,
    frames: &[
        frame!(
            CharacterIndex::One => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Two => [Segment::MiddleLeft, Segment::TopRightDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::Two => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Three => [Segment::MiddleLeft, Segment::TopRightDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::TopRightDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::Four => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
        ),
        frame!(),
        frame!(
            CharacterIndex::One => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::BottomHorizontal],
            CharacterIndex::Two => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::MiddleLeft, Segment::TopRightDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::BottomHorizontal],
            CharacterIndex::Three => [Segment::TopLeftDiagonal, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
    ],
};

pub const PARTIALLY_CLOUDY: Animation = Animation {
    duration: 750,
    frames: &[
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft],
            CharacterIndex::Two => [Segment::TopHorizontal],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft],
            CharacterIndex::Two => [Segment::MiddleLeft],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft, Segment::BottomRightDiagonal],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopLeftVertical, Segment::TopMiddleVertical, Segment::TopLeftDiagonal, Segment::TopRightDiagonal, Segment::MiddleLeft, Segment::BottomLeftVertical],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
    ],
};

pub const FOG: Animation = Animation {
    duration: 2500,
    frames: &[
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal],
            CharacterIndex::Two => [Segment::TopHorizontal],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal, Segment::MiddleRight],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::BottomHorizontal],
            CharacterIndex::Two => [Segment::TopHorizontal],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal, Segment::TopHorizontal, Segment::MiddleLeft],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal, Segment::MiddleRight],
        ),
        frame!(
            CharacterIndex::One => [Segment::BottomHorizontal],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::BottomHorizontal, Segment::MiddleLeft, Segment::MiddleRight],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal, Segment::TopHorizontal, Segment::MiddleLeft],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal, Segment::TopHorizontal, Segment::MiddleRight],
        ),
        frame!(
            CharacterIndex::One => [Segment::BottomHorizontal, Segment::MiddleLeft, Segment::MiddleRight],
            CharacterIndex::Two => [Segment::BottomHorizontal, Segment::MiddleLeft, Segment::MiddleRight],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal, Segment::TopHorizontal, Segment::MiddleLeft],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal, Segment::TopHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::MiddleLeft, Segment::MiddleRight],
            CharacterIndex::Two => [Segment::BottomHorizontal, Segment::MiddleLeft, Segment::MiddleRight],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal, Segment::TopHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::MiddleLeft, Segment::MiddleRight],
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
        frame!(
            CharacterIndex::Three => [Segment::BottomLeftDiagonal, Segment::MiddleRight, Segment::BottomHorizontal],
            CharacterIndex::Four => [Segment::MiddleLeft, Segment::BottomRightDiagonal, Segment::BottomHorizontal],
        ),
    ],
};

pub const QUESTION_MARKS: Animation = Animation {
    duration: 1000,
    frames: &[
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal],
            CharacterIndex::Two => [Segment::TopHorizontal],
            CharacterIndex::Three => [Segment::TopHorizontal],
            CharacterIndex::Four => [Segment::TopHorizontal],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopRightVertical],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::TopRightVertical],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::TopRightVertical],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::TopRightVertical],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::MiddleRight],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::MiddleRight],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::MiddleRight],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::MiddleRight],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
        ),
        frame!(
            CharacterIndex::One => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Two => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Three => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
            CharacterIndex::Four => [Segment::TopHorizontal, Segment::TopRightVertical, Segment::BottomMiddleVertical],
        ),
    ],
};

pub fn build_frame_bytes(
    animation: &Animation,
    frame_index: usize,
) -> Result<[u8; 17], DisplayError> {
    let mut frame_bytes = [0u8; 17];
    for character_segment in animation.frames[frame_index].character_segments {
        let (byte_index, byte) =
            display::segment_to_frame_byte(character_segment.character, character_segment.segment)?;
        frame_bytes[byte_index] |= byte;
    }

    Ok(frame_bytes)
}
