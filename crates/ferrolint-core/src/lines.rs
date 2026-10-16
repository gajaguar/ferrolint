use crate::{Position, Range};

/// Maps byte offsets of one source text to lines and columns.
#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => line_starts.push(offset_u32(i + 1)),
                b'\r' if bytes.get(i + 1) != Some(&b'\n') => line_starts.push(offset_u32(i + 1)),
                _ => {}
            }
            i += 1;
        }
        Self { line_starts }
    }

    /// The 1-based line holding `offset`.
    pub fn line(&self, offset: u32) -> u32 {
        let index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        offset_u32(index) + 1
    }

    /// The byte offset where 1-based `line` starts, if the line exists.
    pub fn line_start(&self, line: u32) -> Option<u32> {
        let index = usize::try_from(line.checked_sub(1)?).ok()?;
        self.line_starts.get(index).copied()
    }

    /// The position of `offset` in `text`; an offset past the end clamps to it.
    pub fn position(&self, text: &str, offset: u32) -> Position {
        let offset = offset.min(offset_u32(text.len()));
        let line = self.line(offset);
        let start = self.line_starts[usize::try_from(line - 1).unwrap_or(0)];
        let column = text
            .get(start as usize..offset as usize)
            .map_or(offset - start, |prefix| offset_u32(prefix.chars().count()));
        Position {
            offset,
            line,
            column: column + 1,
        }
    }

    /// The range between two byte offsets of `text`.
    pub fn range(&self, text: &str, start: u32, end: u32) -> Range {
        Range {
            start: self.position(text, start),
            end: self.position(text, end),
        }
    }
}

/// Source files over 4 GiB are not supported; offsets saturate.
fn offset_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_one_based_and_count_characters() {
        let text = "a = 1\né = 2\r\nb";
        let index = LineIndex::new(text);
        assert_eq!(
            index.position(text, 0),
            Position {
                offset: 0,
                line: 1,
                column: 1
            }
        );
        let b = u32::try_from(text.find('b').unwrap()).unwrap();
        assert_eq!(index.position(text, b).line, 3);
        let two = u32::try_from(text.find('2').unwrap()).unwrap();
        assert_eq!(
            index.position(text, two),
            Position {
                offset: two,
                line: 2,
                column: 5
            }
        );
    }

    #[test]
    fn line_start_finds_each_line() {
        let index = LineIndex::new("x\ny\n");
        assert_eq!(index.line_start(2), Some(2));
        assert_eq!(index.line_start(9), None);
    }
}
