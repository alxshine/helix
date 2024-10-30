use std::borrow::Cow;

use helix_core::{regex::Regex, shellwords::Shellwords};

pub fn expand_in_commands(_cx: &mut crate::commands::Context, args: &Vec<Cow<str>>) -> String {
    log::warn!("expanding in commands");

    for a in args {
        log::warn!("{}", a);
    }

    "".to_string()
}

pub fn expand_string(view: &helix_view::View, doc: &helix_view::Document, input: &str) -> String {
    let (ret, _) = expand_string_with_prompts(view, doc, input);
    ret
}

pub fn expand_string_with_prompts(
    view: &helix_view::View,
    doc: &helix_view::Document,
    input: &str,
) -> (std::string::String, Vec<std::string::String>) {
    let re = Regex::new(r"%\{([^\}]+)\}").expect("Constant regex, never fails");
    // go through all captures
    // - start of capture > current index in ret
    // - current index += diff
    //
    // expand the expression
    // append to ret
    // update current_index
    //
    let mut ret = String::from("");
    let mut prompts: Vec<String> = Vec::new();
    let mut input_index = 0;
    let mut last_end = 0;
    re.captures_iter(input)
        .map(|c| c.get(0).expect("should always have index 0"))
        .for_each(|c| {
            // check to see if we need to copy from input string
            if input_index < c.start() {
                let diff = c.start() - input_index;
                ret.push_str(&input[input_index..c.start()]);
                input_index += diff
            }

            // expand the expression
            let str = c.as_str();
            let inner_candidate = &str[2..str.len() - 1];
            _expand_single_exp(view, doc, inner_candidate, &mut ret, &mut prompts);

            input_index += c.end() - c.start(); // increment index by length in input
            last_end = c.end(); // remember end of last capture
        });

    // check if we need to append something after the last expansion
    if last_end < input.len() {
        ret.push_str(&input[last_end..]);
    }

    (ret, prompts)
}

fn _expand_single_exp(
    view: &helix_view::View,
    doc: &helix_view::Document,
    expansion_candidate: &str,
    ret: &mut String,
    prompts: &mut Vec<String>,
) {
    let shellwords = Shellwords::from(expansion_candidate);
    log::warn!("{:?}", shellwords.words());
    if let Some(first_word) = shellwords.words().first() {
        let expanded = match first_word.as_ref() {
            "prompt" => {
                let text = shellwords.words().join(" ");
                prompts.push(text);
                "%{prompt}".to_string()
            }
            "basename" => doc
                .path()
                .and_then(|x| x.file_name().and_then(|y| y.to_str()))
                .unwrap_or(helix_view::document::SCRATCH_BUFFER_NAME)
                .to_owned(),
            "filename" => doc
                .path()
                .and_then(|x| x.to_str())
                .unwrap_or(helix_view::document::SCRATCH_BUFFER_NAME)
                .to_owned(),
            "selection" => {
                let selection = doc
                    .selection(view.id)
                    .primary()
                    .fragment(doc.text().slice(..));
                // HACK: we can't return selection because it's not tied to a lifetime object :/
                // therefore we have to manipulate ret in here and return ""
                ret.push_str(&selection);
                String::new()
            }
            "git_repo" => helix_loader::find_workspace()
                .0
                .to_str()
                .unwrap_or("")
                .to_owned(),
            // TODO: add the remaining expansions here:
            // https://github.com/tdaron/helix/blob/command-expansion/helix-view/src/editor/variable_expansion.rs
            _ => {
                log::warn!(
                    "VAREXP: encountered unknown expansion {}, full str: {}",
                    first_word,
                    expansion_candidate
                );
                "".to_owned()
            }
        };
        ret.push_str(expanded.as_str());
    } else {
        log::warn!("empty expansion encountered");
        return;
    }
}
