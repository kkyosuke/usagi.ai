use crate::presentation::ui::modal::{
    draw_box_bottom, draw_box_divider, draw_box_title, draw_box_top,
};
use anyhow::Result;
use console::{measure_text_width, style, Term};

pub struct InputModal<'a> {
    pub title: &'a str,
    pub input: &'a str,
    pub cursor_pos: usize,
}

impl<'a> InputModal<'a> {
    pub fn new(title: &'a str, input: &'a str, cursor_pos: usize) -> Self {
        Self {
            title,
            input,
            cursor_pos,
        }
    }

    pub fn render(&self, term: &Term, height: usize, width: usize, x_offset: usize) -> Result<()> {
        let popup_x = x_offset + 4;
        let popup_width = width.saturating_sub(popup_x).saturating_sub(2);

        // タイトル行 + 区切り線 + 入力行
        let box_height = 3;
        let mut offset = 4 + box_height;

        // 上枠
        draw_box_top(term, popup_x, height.saturating_sub(offset), popup_width)?;
        offset -= 1;

        // タイトル
        draw_box_title(
            term,
            popup_x,
            height.saturating_sub(offset),
            popup_width,
            self.title,
        )?;
        offset -= 1;

        // 区切り線
        draw_box_divider(term, popup_x, height.saturating_sub(offset), popup_width)?;
        offset -= 1;

        // 入力フィールド
        term.move_cursor_to(popup_x, height.saturating_sub(offset))?;

        let max_input_width = popup_width.saturating_sub(3);
        let (display_input, cursor_column) =
            visible_input(self.input, self.cursor_pos, max_input_width);
        let padding = " ".repeat(max_input_width.saturating_sub(measure_text_width(display_input)));
        let content = format!("│  {display_input}{padding} │");
        term.write_str(&style(content).cyan().to_string())?;

        offset -= 1;

        // 下枠
        draw_box_bottom(term, popup_x, height.saturating_sub(offset), popup_width)?;

        // カーソルを実際の入力位置に移動させる（呼び出し側で制御しやすいように、描画の最後にカーソルを合わせる）
        let cursor_x = popup_x + 3 + cursor_column;
        let cursor_y = height.saturating_sub(offset + 1); // 入力行のY座標
        term.move_cursor_to(cursor_x, cursor_y)?;
        term.show_cursor()?;
        term.flush()?;

        Ok(())
    }
}

/// Scroll on UTF-8 boundaries and measure terminal columns, not bytes.
fn visible_input(input: &str, cursor_pos: usize, width: usize) -> (&str, usize) {
    let cursor = input
        .char_indices()
        .nth(cursor_pos)
        .map_or(input.len(), |(index, _)| index);
    let mut start = 0;
    while measure_text_width(&input[start..cursor]) > width {
        start += input[start..].chars().next().unwrap().len_utf8();
    }
    let mut end = start;
    for (index, ch) in input[start..].char_indices() {
        let next = start + index + ch.len_utf8();
        if measure_text_width(&input[start..next]) > width {
            break;
        }
        end = next;
    }
    (
        &input[start..end],
        measure_text_width(&input[start..cursor]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolls_japanese_input_without_splitting_utf8() {
        assert_eq!(visible_input("あいう", 3, 3), ("う", 2));
        assert_eq!(visible_input("日本語abc", 6, 5), ("語abc", 5));
    }

    #[test]
    fn keeps_cursor_visible_when_editing_in_the_middle() {
        assert_eq!(visible_input("abcdef", 2, 3), ("abc", 2));
        assert_eq!(visible_input("a日本b", 2, 4), ("a日", 3));
        assert_eq!(visible_input("abcdef", 5, 3), ("cde", 3));
    }

    #[test]
    fn handles_empty_and_narrow_inputs() {
        assert_eq!(visible_input("", 100, 4), ("", 0));
        assert_eq!(visible_input("日本", 2, 0), ("", 0));
        assert_eq!(visible_input("abc", 100, 2), ("bc", 2));
    }
}
