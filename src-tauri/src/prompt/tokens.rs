//! token 估算（设计文档 §6.6.6）。
//!
//! **这不是精确计数**，而是一个刻意保守的估计。目的不是省钱，
//! 而是让用户在把提示词粘进 AI 之前知道「这大概是 200 字还是 20000 字」——
//! 超长的提示词不但贵，还会让模型忽略中间部分。
//!
//! 估算规则按主流分词器的经验值：
//! - 中文：约 1 token / 字（BPE 对汉字多为单字成 token）
//! - ASCII 字母数字：约 4 字符 / token
//! - 其他（标点、符号）：约 2 字符 / token
//!
//! 刻意**向上取整**：宁可高估让用户提前发现，也不要低估导致超限被截断。

/// 估算一段文本的 token 数。
pub fn estimate(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut ascii_word = 0usize;
    let mut other = 0usize;

    for character in text.chars() {
        if is_cjk(character) {
            cjk += 1;
        } else if character.is_ascii_alphanumeric() || character == ' ' || character == '\n' {
            ascii_word += 1;
        } else {
            other += 1;
        }
    }

    // 向上取整：4 个 ASCII 字符算 1 个 token，不足 4 个也算 1 个
    let ascii_tokens = ascii_word.div_ceil(4);
    let other_tokens = other.div_ceil(2);

    cjk + ascii_tokens + other_tokens
}

/// 汉字与常见中日韩标点。
fn is_cjk(character: char) -> bool {
    matches!(character as u32,
        0x3000..=0x303F   // CJK 标点
        | 0x4E00..=0x9FFF // 汉字基本区
        | 0xFF00..=0xFFEF // 全角字符
    )
}

/// 粗略的字符数（用户对「多长」的直觉更接近字符数）。
pub fn char_count(text: &str) -> usize {
    text.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_is_roughly_one_token_per_character() {
        let tokens = estimate("价格突破均线");
        assert_eq!(tokens, 6);
    }

    #[test]
    fn ascii_is_roughly_four_characters_per_token() {
        assert_eq!(estimate("abcdefgh"), 2);
        // 不足 4 个也要算 1 个：向上取整
        assert_eq!(estimate("a"), 1);
        assert_eq!(estimate("abcde"), 2);
    }

    #[test]
    fn mixed_text_adds_up() {
        // 4 个汉字 + "abcd"（1 token）+ ":"（1 token）
        let tokens = estimate("开仓均价:abcd");
        assert_eq!(tokens, 4 + 1 + 1);
    }

    #[test]
    fn estimate_is_never_zero_for_non_empty_text() {
        for text in ["a", "。", "\n", "x"] {
            assert!(estimate(text) >= 1, "{text:?} 应至少算 1 个 token");
        }
    }

    #[test]
    fn char_count_handles_multibyte() {
        assert_eq!(char_count("abc中文"), 5);
        assert_eq!(char_count(""), 0);
    }
}
