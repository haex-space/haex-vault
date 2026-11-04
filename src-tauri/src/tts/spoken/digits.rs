//! Digits that are no count: telephone numbers in groups ("0711-123456", "555-123-4567") and
//! tokens haex cannot read as a whole ("1.2.3", "2:1", "11/04/2026"), which stay as written
//! (FR-022b).

use super::Reader;

impl Reader<'_> {
    /// Digit groups joined by dashes, said digit by digit with a pause between the groups: three
    /// groups or more, or two when the first has a leading zero. Two groups without one are a
    /// range ("10-12").
    pub(super) fn digit_groups_at(&mut self, i: usize) -> Option<usize> {
        let mut groups = vec![(i, self.digit_run(i))];
        while let Some(&(_, end)) = groups.last() {
            if self.char(end) != Some('-') || !self.is_digit(end + 1) {
                break;
            }
            groups.push((end + 1, self.digit_run(end + 1)));
        }
        let (start, first_end) = groups[0];
        let leading_zero = first_end - start > 1 && self.chars[start] == '0';
        let end = groups.last()?.1;
        if groups.len() < 2 || (groups.len() == 2 && !leading_zero) || !self.ends_token(end) {
            return None;
        }
        let said: Vec<String> = groups
            .iter()
            .map(|&(from, to)| {
                self.digit_by_digit(&self.chars[from..to].iter().collect::<String>())
            })
            .collect();
        self.out.push_str(&said.join(", "));
        Some(end)
    }

    /// Copies the token of digits and inner separators at `i` unchanged; returns its end.
    pub(super) fn keep_as_written(&mut self, i: usize) -> usize {
        let mut end = i;
        while let Some(c) = self.char(end) {
            let inner = matches!(c, '.' | ',' | ':' | '/') && self.is_digit(end + 1);
            if !(c.is_ascii_digit() || inner) {
                break;
            }
            end += 1;
        }
        self.out.extend(&self.chars[i..end]);
        end
    }
}
