use std::borrow::Cow;

use helix_core::{regex::Regex, shellwords::Shellwords};

pub fn expand_and_execute(cx: &mut crate::commands::Context, name: &String, args: &Vec<Cow<str>>) {
    let input = args.join(" ");
    _expand_and_exec(cx, name, &input);
}

fn _expand_and_exec(cx: &mut crate::commands::Context, name: &String, input: &str) {
    // match for prompt
    let re = Regex::new(r"%\{prompt ([^\}]+)\}").expect("constant regex, never fails");
    if let Some(prompt_match) = re.captures(input) {
        // if found, show prompt
        let prompt_text = prompt_match
            .get(1)
            .expect("constant regex should have at least 1 capture group")
            .as_str()
            .to_owned();

        let cap = prompt_match.get(0).expect("always has group 0");
        let before = input[..cap.start()].to_owned();
        let after = input[cap.end()..].to_owned();
        let name = name.to_owned();

        let prompt = crate::ui::Prompt::new(
            Cow::from(prompt_text + ": "),
            Some('w'),
            |_, _| Vec::new(), // completion
            move |cx: &mut crate::compositor::Context,
                  prompt_response: &str,
                  event: crate::ui::PromptEvent| {
                // due to lifetime and borrow restrictions we have to duplicate this code in both if branches
                if event == crate::ui::PromptEvent::Validate {
                    let expanded = String::from(before.as_str()) + prompt_response + after.as_str();
                    let (view, doc) = current_ref!(cx.editor);
                    let expanded = expand_string(view, doc, expanded.as_str());

                    let correctly_typed_args: Vec<Cow<str>> = expanded
                        .split(' ')
                        .map(String::from)
                        .map(Cow::from)
                        .collect();

                    log::warn!("constructed args: {}", expanded);

                    if let Some(command) =
                        crate::commands::typed::TYPABLE_COMMAND_MAP.get(name.as_str())
                    {
                        let mut cx = crate::compositor::Context {
                            editor: cx.editor,
                            jobs: cx.jobs,
                            scroll: None,
                        };
                        if let Err(e) = (command.fun)(
                            &mut cx,
                            &correctly_typed_args[..],
                            crate::ui::PromptEvent::Validate,
                        ) {
                            cx.editor.set_error(format!("{}", e));
                        }
                    }
                }
            },
        );
        cx.push_layer(Box::new(prompt));
        return;
    } else {
        // repeat execution here, but with input
        let (view, doc) = current_ref!(cx.editor);
        let expanded = expand_string(view, doc, input);

        let correctly_typed_args: Vec<Cow<str>> = expanded
            .split(' ')
            .map(String::from)
            .map(Cow::from)
            .collect();

        if let Some(command) = crate::commands::typed::TYPABLE_COMMAND_MAP.get(name.as_str()) {
            let mut cx = crate::compositor::Context {
                editor: cx.editor,
                jobs: cx.jobs,
                scroll: None,
            };
            if let Err(e) = (command.fun)(
                &mut cx,
                &correctly_typed_args[..],
                crate::ui::PromptEvent::Validate,
            ) {
                cx.editor.set_error(format!("{}", e));
            }
        }
    }
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
    let mut ret = String::new();
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
    if let Some(first_word) = shellwords.words().first() {
        let expanded = match first_word.as_ref() {
            "prompt" => {
                let text = shellwords.words()[1..].join(" ");
                prompts.push(text);
                "%{prompt}".to_string()
            }
            // DISCLAIMER: these are from tdaron's fork until his PR is merged:
            // https://github.com/tdaron/helix/blob/command-expansion/helix-view/src/editor/variable_expansion.rs
            "basename" => doc
                .path()
                .and_then(|it| it.file_name().and_then(|it| it.to_str()))
                .unwrap_or(helix_view::document::SCRATCH_BUFFER_NAME)
                .to_owned(),
            "filename" => doc
                .path()
                .and_then(|it| it.to_str())
                .unwrap_or(helix_view::document::SCRATCH_BUFFER_NAME)
                .to_owned(),
            "filename:git_rel" => {
                // This will get git repo root or cwd if not inside a git repo.
                let workspace_path = helix_loader::find_workspace().0;
                doc.path()
                    .and_then(|p| p.strip_prefix(workspace_path).unwrap_or(p).to_str())
                    .unwrap_or(helix_view::document::SCRATCH_BUFFER_NAME)
                    .to_owned()
            }
            "filename:rel" => {
                let cwd = helix_stdx::env::current_working_dir();
                doc.path()
                    .and_then(|p| p.strip_prefix(cwd).unwrap_or(p).to_str())
                    .unwrap_or(helix_view::document::SCRATCH_BUFFER_NAME)
                    .to_owned()
            }
            "dirname" => doc
                .path()
                .and_then(|p| p.parent())
                .and_then(std::path::Path::to_str)
                .unwrap_or(helix_view::document::SCRATCH_BUFFER_NAME)
                .to_owned(),
            "git_repo" => helix_loader::find_workspace()
                .0
                .to_str()
                .unwrap_or("")
                .to_owned(),
            "cwd" => helix_stdx::env::current_working_dir()
                .to_str()
                .unwrap()
                .to_owned(),
            "linenumber" => (doc
                .selection(view.id)
                .primary()
                .cursor_line(doc.text().slice(..))
                + 1)
            .to_string(),
            // "cursorcolumn" => (coords_at_pos( // FIXME: would have to add actual editor as well
            //     doc.text().slice(..),
            //     doc.selection(view.id)
            //         .primary()
            //         .cursor(doc.text().slice(..)),
            // )
            // .col + 1)
            //     .to_string(),
            "lang" => doc.language_name().unwrap_or("text").to_string(),
            "ext" => doc
                .relative_path()
                .and_then(|p| p.extension()?.to_os_string().into_string().ok())
                .unwrap_or_default(),
            "selection" => doc
                .selection(view.id)
                .primary()
                .fragment(doc.text().slice(..))
                .to_string(),
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
