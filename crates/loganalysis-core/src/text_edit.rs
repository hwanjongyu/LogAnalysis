/// Inserts a substring at the given Unicode character index and returns the new character cursor index.
pub fn insert_at_char_index(s: &mut String, idx: usize, insert: &str) -> usize {
    let mut byte_idx = s.len();
    for (char_idx, (b_idx, _)) in s.char_indices().enumerate() {
        if char_idx == idx {
            byte_idx = b_idx;
            break;
        }
    }
    s.insert_str(byte_idx, insert);
    idx + insert.chars().count()
}

/// Removes the character immediately preceding the given Unicode character index (Backspace)
/// and returns the updated cursor index.
pub fn remove_before_char_index(s: &mut String, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let target = idx - 1;
    let mut remove_range = None;
    for (char_count, (b_idx, ch)) in s.char_indices().enumerate() {
        if char_count == target {
            remove_range = Some(b_idx..b_idx + ch.len_utf8());
            break;
        }
    }
    if let Some(range) = remove_range {
        s.drain(range);
        target
    } else {
        idx
    }
}

/// Removes the character at the given Unicode character index (Delete key).
pub fn remove_at_char_index(s: &mut String, idx: usize) {
    let mut remove_range = None;
    for (char_count, (b_idx, ch)) in s.char_indices().enumerate() {
        if char_count == idx {
            remove_range = Some(b_idx..b_idx + ch.len_utf8());
            break;
        }
    }
    if let Some(range) = remove_range {
        s.drain(range);
    }
}

/// Deletes the previous word before the character cursor index and returns the new cursor index.
pub fn delete_previous_word_at(s: &mut String, idx: usize) -> usize {
    let chars: Vec<char> = s.chars().collect();
    if idx == 0 || chars.is_empty() {
        return 0;
    }
    let cur = idx.min(chars.len());
    let mut target = cur;

    // Step 1: skip trailing whitespace before cursor
    while target > 0 && chars[target - 1].is_whitespace() {
        target -= 1;
    }
    // Step 2: remove word characters before cursor
    while target > 0 && !chars[target - 1].is_whitespace() {
        target -= 1;
    }

    if target < cur {
        let start_byte = chars[..target].iter().map(|c| c.len_utf8()).sum::<usize>();
        let end_byte = start_byte + chars[target..cur].iter().map(|c| c.len_utf8()).sum::<usize>();
        s.drain(start_byte..end_byte);
    }

    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_insert_char_index() {
        let mut s = "helo".to_string();
        let next_pos = insert_at_char_index(&mut s, 2, "l");
        assert_eq!(s, "hello");
        assert_eq!(next_pos, 3);

        // Beginning insertion
        let mut s2 = "world".to_string();
        let next_pos2 = insert_at_char_index(&mut s2, 0, "hello ");
        assert_eq!(s2, "hello world");
        assert_eq!(next_pos2, 6);

        // Unicode / CJK insertion
        let mut s3 = "가다".to_string();
        let next_pos3 = insert_at_char_index(&mut s3, 1, "나");
        assert_eq!(s3, "가나다");
        assert_eq!(next_pos3, 2);
    }

    #[test]
    fn test_cursor_remove_before_char_index() {
        let mut s = "hello".to_string();
        let cur = remove_before_char_index(&mut s, 3); // deletes 'l' at index 2
        assert_eq!(s, "helo");
        assert_eq!(cur, 2);

        // Backspace at 0 does nothing
        let cur0 = remove_before_char_index(&mut s, 0);
        assert_eq!(s, "helo");
        assert_eq!(cur0, 0);
    }

    #[test]
    fn test_cursor_remove_at_char_index() {
        let mut s = "hello".to_string();
        remove_at_char_index(&mut s, 1); // deletes 'e' at index 1
        assert_eq!(s, "hllo");

        // Delete at end does nothing
        remove_at_char_index(&mut s, 4);
        assert_eq!(s, "hllo");
    }

    #[test]
    fn test_cursor_delete_previous_word_at() {
        let mut s = "fatal error".to_string();
        let cur = delete_previous_word_at(&mut s, 11);
        assert_eq!(s, "fatal ");
        assert_eq!(cur, 6);

        let cur2 = delete_previous_word_at(&mut s, 6);
        assert_eq!(s, "");
        assert_eq!(cur2, 0);

        // Word with trailing whitespace
        let mut s3 = "hello   world   ".to_string();
        let len3 = s3.chars().count();
        let cur3 = delete_previous_word_at(&mut s3, len3);
        assert_eq!(s3, "hello   ");
        assert_eq!(cur3, 8);

        // Unicode / CJK word deletion
        let mut s4 = "로그 분석기".to_string();
        let len4 = s4.chars().count();
        let cur4 = delete_previous_word_at(&mut s4, len4);
        assert_eq!(s4, "로그 ");
        assert_eq!(cur4, 3);
    }
}
