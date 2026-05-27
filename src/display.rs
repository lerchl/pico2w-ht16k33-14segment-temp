use heapless::Vec;

use crate::glyph::glyph;

// byte: 0b12345678
//         ||||||||
//         |||||||+-- bit 8 (LSB)
//         ||||||+--- bit 7
//         |||||+---- bit 6
//         ||||+----- bit 5
//         |||+------ bit 4
//         ||+------- bit 3
//         |+-------- bit 2
//         +--------- bit 1 (MSB)
//
// Frame index (3 and 4|5 and 6|7 and 8|9 and 10) represent some
// segments of the (first|second|third|fourth) character on the
// display. The first number indicates whether it's the first or the
// second byte. The second number is which bit of that byte. Some bits
// do nothing.
//
//         1,4
//     ___________
//    |\ 2,3|    /|
//    | |   |   | |
//    |  \  |  /  |
// 2,2|   | | |   |1,2
//    | 1,1\|/    |
//    |     |     |
//  2,7----- -----2,8
//    |     |     |
//    | 2,4/|\    |
//    |   | | |   |
// 1,5|  /  |  \  |1,3
//    | |   |   | |
//    |/ 2,5|    \|
//     ‾‾‾‾‾‾‾‾‾‾‾
//         2,6
//
// Frame index 11 and 12 represent the remaining segments all over the
// four characters and the two middle dots. The first number indicates
// whether it's the first or the second byte, frame index 11 or 12. The
// second number is which bit of that byte. Some bits do nothing.
//
//
//     ___________        ___________         ___________        ___________
//    |\    |    /|      |\    |    /|       |\    |    /|      |\    |    /|
//    | |   |   | |      | |   |   | |       | |   |   | |      | |   |   | |
//    |  \  |  /  |      |  \  |  /  |  1,1  |  \  |  /  |      |  \  |  /  |
//    |   | | |   |      |   | | |   |   O   |   | | |   |      |   | | |   |
//    |    \|/1,4 |      |    \|/1,2 |       |    \|/1,3 |      |    \|/2,6 |
//    |     |     |      |     |     |       |     |     |      |     |     |
//     ----- -----        ----- -----         ----- -----        ----- -----
//    |     |     |      |     |     |       |     |     |      |     |     |
//    |    /|\1,5 |      |    /|\2,2 |       |    /|\2,7 |      |    /|\2,8 |
//    |   | | |   |      |   | | |   |   O   |   | | |   |      |   | | |   |
//    |  /  |  \  |      |  /  |  \  |  2,3  |  /  |  \  |      |  /  |  \  |
//    | |   |   | |      | |   |   | |       | |   |   | |      | |   |   | |
//    |/    |    \|      |/    |    \|       |/    |    \|      |/    |    \|
//     ‾‾‾‾‾‾‾‾‾‾‾        ‾‾‾‾‾‾‾‾‾‾‾         ‾‾‾‾‾‾‾‾‾‾‾        ‾‾‾‾‾‾‾‾‾‾‾
//

#[derive(Clone, Copy, Debug)]
pub enum Segment {
    TopHorizontal,
    TopLeftVertical,
    TopLeftDiagonal,
    TopMiddleVertical,
    TopRightDiagonal,
    TopRightVertical,
    MiddleLeft,
    MiddleRight,
    BottomLeftVertical,
    BottomLeftDiagonal,
    BottomMiddleVertical,
    BottomRightDiagonal,
    BottomRightVertical,
    BottomHorizontal,
}

#[derive(Debug)]
pub enum DisplayError {
    InvalidCharacterIndex(usize),
    UnknownGlyph(char),
}

pub struct Character {
    pub index: usize,
    pub segments: Vec<Segment, 14>,
}

pub struct Display {
    pub characters: [Character; 4],
}

impl Display {
    pub fn from_str(s: &str) -> Result<Self, DisplayError> {
        let mut characters = [
            Character {
                index: 1,
                segments: Vec::new(),
            },
            Character {
                index: 2,
                segments: Vec::new(),
            },
            Character {
                index: 3,
                segments: Vec::new(),
            },
            Character {
                index: 4,
                segments: Vec::new(),
            },
        ];

        for (i, c) in s.chars().enumerate().take(4) {
            let segments = glyph(c)?;
            characters[i] = Character {
                index: i + 1,
                segments: segments.iter().copied().collect(),
            };
        }
        Ok(Self { characters })
    }

    pub fn to_frame(&self) -> Result<[u8; 17], DisplayError> {
        let mut frame = [0u8; 17];
        for character in &self.characters {
            for &segment in &character.segments {
                let (index, mask) = segment_to_frame_byte(character.index, segment)?;
                frame[index] |= mask;
            }
        }
        Ok(frame)
    }
}

pub fn segment_to_frame_byte(
    character_index: usize,
    segment: Segment,
) -> Result<(usize, u8), DisplayError> {
    if character_index < 1 || character_index > 4 {
        return Err(DisplayError::InvalidCharacterIndex(character_index));
    }

    let char_first_byte_index = 3 + (character_index - 1) * 2;

    let top_right_diagonal_bitmask = || match character_index {
        1 => 0b00010000,
        2 => 0b01000000,
        3 => 0b00100000,
        4 => 0b00000100,
        _ => unreachable!(),
    };

    let bottom_right_diagonal_bitmask = || match character_index {
        1 => 0b00001000,
        2 => 0b01000000,
        3 => 0b00000010,
        4 => 0b00000001,
        _ => unreachable!(),
    };

    Ok(match segment {
        Segment::TopHorizontal => (char_first_byte_index, 0b00010000),
        Segment::TopLeftVertical => (char_first_byte_index + 1, 0b01000000),
        Segment::TopLeftDiagonal => (char_first_byte_index, 0b10000000),
        Segment::TopMiddleVertical => (char_first_byte_index + 1, 0b00100000),
        Segment::TopRightDiagonal => (
            if character_index == 4 { 12 } else { 11 },
            top_right_diagonal_bitmask(),
        ),
        Segment::TopRightVertical => (char_first_byte_index, 0b01000000),
        Segment::MiddleLeft => (char_first_byte_index + 1, 0b00000010),
        Segment::MiddleRight => (char_first_byte_index + 1, 0b00000001),
        Segment::BottomLeftVertical => (char_first_byte_index, 0b00001000),
        Segment::BottomLeftDiagonal => (char_first_byte_index + 1, 0b00010000),
        Segment::BottomMiddleVertical => (char_first_byte_index + 1, 0b00001000),
        Segment::BottomRightDiagonal => (
            if character_index == 1 { 11 } else { 12 },
            bottom_right_diagonal_bitmask(),
        ),
        Segment::BottomRightVertical => (char_first_byte_index, 0b00100000),
        Segment::BottomHorizontal => (char_first_byte_index + 1, 0b00000100),
    })
}
