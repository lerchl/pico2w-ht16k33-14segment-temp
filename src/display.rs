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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CharacterIndex {
    One,
    Two,
    Three,
    Four,
}

impl TryFrom<usize> for CharacterIndex {
    type Error = ();

    fn try_from(index: usize) -> Result<Self, Self::Error> {
        match index {
            0 => Ok(CharacterIndex::One),
            1 => Ok(CharacterIndex::Two),
            2 => Ok(CharacterIndex::Three),
            3 => Ok(CharacterIndex::Four),
            _ => Err(()),
        }
    }
}

impl CharacterIndex {
    pub fn first_byte_index(&self) -> usize {
        match self {
            CharacterIndex::One => 3,
            CharacterIndex::Two => 5,
            CharacterIndex::Three => 7,
            CharacterIndex::Four => 9,
        }
    }
}

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
    // Is logged
    #[allow(dead_code)]
    UnknownGlyph(char),
}

#[derive(Debug)]
pub struct Character {
    pub index: CharacterIndex,
    pub segments: Vec<Segment, 14>,
}

pub struct Display {
    pub characters: [Character; 4],
}

impl Display {
    pub fn from_str(s: &str) -> Result<Self, DisplayError> {
        let characters = s
            .chars()
            .enumerate()
            .take(4)
            .map(|(index, char)| {
                let segments = glyph(char)?;
                // CharacterIndex::try_from cannot fail due to .take(4) above!
                Ok(Character {
                    index: CharacterIndex::try_from(index).unwrap(),
                    segments: segments.iter().copied().collect(),
                })
            })
            .collect::<Result<heapless::Vec<Character, 4>, DisplayError>>()?;

        Ok(Self {
            characters: characters.into_array().unwrap(),
        })
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
    character_index: CharacterIndex,
    segment: Segment,
) -> Result<(usize, u8), DisplayError> {
    let char_first_byte_index = character_index.first_byte_index();

    let top_right_diagonal_bitmask = || match character_index {
        CharacterIndex::One => 0b00010000,
        CharacterIndex::Two => 0b01000000,
        CharacterIndex::Three => 0b00100000,
        CharacterIndex::Four => 0b00000100,
    };

    let bottom_right_diagonal_bitmask = || match character_index {
        CharacterIndex::One => 0b00001000,
        CharacterIndex::Two => 0b01000000,
        CharacterIndex::Three => 0b00000010,
        CharacterIndex::Four => 0b00000001,
    };

    Ok(match segment {
        Segment::TopHorizontal => (char_first_byte_index, 0b00010000),
        Segment::TopLeftVertical => (char_first_byte_index + 1, 0b01000000),
        Segment::TopLeftDiagonal => (char_first_byte_index, 0b10000000),
        Segment::TopMiddleVertical => (char_first_byte_index + 1, 0b00100000),
        Segment::TopRightDiagonal => (
            if character_index == CharacterIndex::Four {
                12
            } else {
                11
            },
            top_right_diagonal_bitmask(),
        ),
        Segment::TopRightVertical => (char_first_byte_index, 0b01000000),
        Segment::MiddleLeft => (char_first_byte_index + 1, 0b00000010),
        Segment::MiddleRight => (char_first_byte_index + 1, 0b00000001),
        Segment::BottomLeftVertical => (char_first_byte_index, 0b00001000),
        Segment::BottomLeftDiagonal => (char_first_byte_index + 1, 0b00010000),
        Segment::BottomMiddleVertical => (char_first_byte_index + 1, 0b00001000),
        Segment::BottomRightDiagonal => (
            if character_index == CharacterIndex::One {
                11
            } else {
                12
            },
            bottom_right_diagonal_bitmask(),
        ),
        Segment::BottomRightVertical => (char_first_byte_index, 0b00100000),
        Segment::BottomHorizontal => (char_first_byte_index + 1, 0b00000100),
    })
}
