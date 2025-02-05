use std::collections::HashSet;

use crate::chunking::{
    text_document::{Position, Range},
    types::OutlineNode,
};
use async_recursion::async_recursion;
use futures::{stream, StreamExt};
use thiserror::Error;

use super::helpers::{guess_content, ProbableFileKind};

#[derive(Debug, Error)]
pub enum UserContextError {
    #[error("Unable to read from path: {0}")]
    UnableToReadFromPath(String),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum VariableType {
    File,
    CodeSymbol,
    Selection,
}

impl VariableType {
    pub fn selection(&self) -> bool {
        self == &VariableType::Selection
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct VariableInformation {
    pub start_position: Position,
    pub end_position: Position,
    pub fs_file_path: String,
    pub name: String,
    #[serde(rename = "type")]
    pub variable_type: VariableType,
    pub content: String,
    pub language: String,
}

impl VariableInformation {
    /// Unique identifier is made up for `fs_file_path:start_position:end_position`
    pub fn unique_identifier(&self) -> String {
        format!(
            "{}-{:?}-{:?}",
            self.fs_file_path, self.start_position, self.end_position
        )
    }
    /// Helps create a new custom selection with name provided by the system
    pub fn create_selection(
        range: Range,
        fs_file_path: String,
        name: String,
        content: String,
        language: String,
    ) -> Self {
        Self {
            start_position: range.start_position(),
            end_position: range.end_position(),
            fs_file_path,
            name,
            variable_type: VariableType::Selection,
            content,
            language,
        }
    }

    pub fn create_file(
        range: Range,
        fs_file_path: String,
        name: String,
        content: String,
        language: String,
    ) -> Self {
        Self {
            start_position: range.start_position(),
            end_position: range.end_position(),
            fs_file_path,
            name,
            variable_type: VariableType::File,
            content,
            language,
        }
    }

    pub fn is_selection(&self) -> bool {
        self.variable_type == VariableType::Selection
    }

    pub fn is_file(&self) -> bool {
        self.variable_type == VariableType::File
    }

    pub fn is_code_symbol(&self) -> bool {
        self.variable_type == VariableType::CodeSymbol
    }

    pub fn to_xml(self) -> String {
        let variable_name = self.name;
        let location = format!(
            "{}:{}-{}",
            self.fs_file_path,
            self.start_position.line(),
            self.end_position.line(),
        );
        let content = self.content;
        let language = self.language;

        match self.variable_type {
            VariableType::CodeSymbol => {
                format!(
                    r#"<selection_item>
<symbol>
<name>
{variable_name}
</name>
<type>
Code Symbol
</type>
<file_path>
{location}
</file_path>
<content>
```{language}
{content}
```
</content>
</selection_item>"#
                )
            }
            VariableType::File => {
                format!(
                    r#"<selection_item>
<file>
<name>
{variable_name}
</name>
<file_path>
{location}
</file_path>
<content>
```{language}
{content}
```
</content>
</file>
</selection_item>"#
                )
            }
            VariableType::Selection => {
                format!(
                    r#"<selection_item>
<user_selection>
<name>
{variable_name}
</name>
<file_path>
{location}
</file_path>
<content>
```{language}
{content}
```
</content>
</user_selection>
</selection_item>"#
                )
            }
        }
    }

    pub fn from_outline_node(outline_node: &OutlineNode) -> Self {
        VariableInformation::create_selection(
            outline_node.range().clone(),
            outline_node.fs_file_path().to_owned(),
            if outline_node.is_class() || outline_node.is_class_definition() {
                format!("Definition for {}", outline_node.name())
            } else {
                format!("Implementation for {}", outline_node.name())
            },
            outline_node.content().content().to_owned(),
            outline_node.content().language().to_owned(),
        )
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FileContentValue {
    pub file_path: String,
    pub file_content: String,
    pub language: String,
}

impl FileContentValue {
    pub fn new(file_path: String, file_content: String, language: String) -> Self {
        Self {
            file_content,
            file_path,
            language,
        }
    }

    pub fn to_xml(self) -> String {
        let language = &self.language;
        let content = &self.file_content;
        let file_path = &self.file_path;
        format!(
            r#"<selection_item>
<file>
<file_path>
{file_path}
</file_path>
<content>
```{language}
{content}
```
</content>
</file>
</selection_item>"#
        )
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct UserContext {
    pub variables: Vec<VariableInformation>,
    pub file_content_map: Vec<FileContentValue>,
    pub terminal_selection: Option<String>,
    // These paths will be absolute and need to be used to get the
    // context of the folders here, we will output it properly
    folder_paths: Vec<String>,
    // These are all hacks for now, we will move them to proper strucutre later on
    is_plan_generation: bool,
    is_plan_execution_until: Option<usize>,
    #[serde(default)]
    is_plan_append: bool,
    #[serde(default)]
    is_plan_drop_from: Option<usize>,
}

impl UserContext {
    pub fn new(
        variables: Vec<VariableInformation>,
        file_content_map: Vec<FileContentValue>,
        terminal_selection: Option<String>,
        folder_paths: Vec<String>,
    ) -> Self {
        Self {
            variables,
            file_content_map,
            terminal_selection,
            folder_paths,
            is_plan_generation: false,
            is_plan_execution_until: None,
            is_plan_append: false,
            is_plan_drop_from: None,
        }
    }

    pub fn add_variables(mut self, variables: Vec<VariableInformation>) -> Self {
        self.variables.extend(variables);
        self
    }

    /// If we are in any part of the plan generation flow over here
    pub fn is_plan_generation_flow(&self) -> bool {
        self.is_plan_append()
            || self.is_plan_execution_until().is_some()
            || self.is_plan_generation()
            || self.is_plan_drop_from().is_some()
    }

    pub fn is_plan_append(&self) -> bool {
        self.is_plan_append
    }

    pub fn is_plan_execution_until(&self) -> Option<usize> {
        self.is_plan_execution_until
    }

    pub fn is_plan_generation(&self) -> bool {
        self.is_plan_generation
    }

    pub fn is_plan_drop_from(&self) -> Option<usize> {
        self.is_plan_drop_from
    }

    pub fn update_file_content_map(
        mut self,
        file_path: String,
        file_content: String,
        language: String,
    ) -> Self {
        self.file_content_map.push(FileContentValue {
            file_content,
            file_path,
            language,
        });
        self
    }

    pub fn folder_paths(&self) -> Vec<String> {
        self.folder_paths.to_vec()
    }

    pub fn is_empty(&self) -> bool {
        self.variables.is_empty() && self.terminal_selection.is_none()
    }

    pub fn file_paths(&self) -> Vec<String> {
        let mut alredy_seen_file_paths: HashSet<String> = Default::default();
        self.file_content_map
            .iter()
            .filter_map(|file_content| {
                if alredy_seen_file_paths.contains(&file_content.file_path) {
                    None
                } else {
                    alredy_seen_file_paths.insert(file_content.file_path.to_owned());
                    Some(file_content.file_path.to_owned())
                }
            })
            .collect::<Vec<_>>()
    }

    /// Grabs the file paths from the variables
    pub fn file_paths_from_variables(&self) -> Vec<String> {
        self.variables
            .iter()
            .map(|variable| variable.fs_file_path.to_owned())
            .collect::<HashSet<String>>()
            .into_iter()
            .collect::<Vec<_>>()
    }

    /// Grabs the user provided context as a string which can be passed to LLMs for code editing
    ///
    /// This also de-duplicates the context as much as possible making this efficient
    /// We cannot trust the file-system but for now this is a decent hack to make that
    pub async fn to_context_string(&self) -> Result<String, UserContextError> {
        let file_paths = self
            .file_content_map
            .iter()
            .map(|file_content| file_content.file_path.to_owned())
            .collect::<Vec<String>>();

        // now we have to read the file contents as a string and pass it to the LLM
        // for output
        println!(
            "user_context::to_context_string::file_paths({})",
            file_paths.to_vec().join(",")
        );
        let mut file_contents = vec![];
        let mut already_seen_files: HashSet<String> = Default::default();
        for file_path in file_paths.into_iter() {
            if already_seen_files.contains(&file_path) {
                continue;
            }
            already_seen_files.insert(file_path.to_owned());
            let contents = tokio::fs::read(&file_path).await;
            if contents.is_err() {
                continue;
            } else {
                let content = String::from_utf8(contents.expect("is_err to hold"));
                if let Ok(content) = content {
                    file_contents.push(format!(
                        r#"FILEPATH: {file_path}
```
{content}
```"#
                    ));
                }
            }
        }
        Ok(file_contents.join("\n"))
    }

    // generats the full xml for the input context so the llm can query from it
    pub async fn to_xml(
        self,
        file_extension_filters: HashSet<String>,
    ) -> Result<String, UserContextError> {
        let variable_prompt = self
            .variables
            .into_iter()
            .map(|variable| variable.to_xml())
            .collect::<Vec<_>>()
            .join("\n");
        // read the file content as well from the file paths which were shared
        // let mut already_seen_files: HashSet<String> = Default::default();
        // let file_prompt = self
        //     .file_content_map
        //     .into_iter()
        //     .filter_map(|file_content| {
        //         let file_path = file_content.file_path.to_owned();
        //         if already_seen_files.contains(file_path.as_str()) {
        //             None
        //         } else {
        //             already_seen_files.insert(file_path.to_owned());
        //             Some(file_content.to_xml())
        //         }
        //     })
        //     .collect::<Vec<_>>()
        //     .join("\n");
        let folder_content = stream::iter(
            self.folder_paths
                .into_iter()
                .map(|folder_path| (folder_path, file_extension_filters.clone())),
        )
        .map(|(folder_path, file_extension_filters)| {
            read_folder_selection(folder_path, file_extension_filters)
        })
        .buffered(1)
        .collect::<Vec<Result<String, UserContextError>>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, UserContextError>>()?
        .join("\n");
        // Now we create the xml string for this
        let mut final_string = "<selection>\n".to_owned();
        final_string.push_str(&variable_prompt);
        final_string.push_str("\n");
        final_string.push_str(&folder_content);
        // final_string.push_str("\n");
        // final_string.push_str(&file_prompt);
        final_string.push_str("\n</selection>");
        Ok(final_string)
    }

    pub fn is_anchored_editing(&self) -> bool {
        self.variables
            .iter()
            .any(|variable| variable.variable_type == VariableType::Selection)
    }

    // we want to carry over the variable information from previous steps, we can
    // literally start with raw updating it based on a refresh from the fs
    // or keeping it as part of the
    pub async fn update_variable_information(
        mut self,
        variables: Vec<VariableInformation>,
    ) -> Self {
        // to keep the variable information updated we have to do the following:
        // make sure that the previous version is kept always
        // we will allow toggles to update the user context
        let additional_variables = variables
            .into_iter()
            .filter_map(|additional_variable| {
                if self
                    .variables
                    .iter()
                    .map(|variable| variable.unique_identifier())
                    .any(|variable_identifier| {
                        variable_identifier == additional_variable.unique_identifier()
                    })
                {
                    None
                } else {
                    Some(additional_variable)
                }
            })
            .collect::<Vec<_>>();
        self.variables.extend(additional_variables);
        self
    }

    /// Merges the user context on the variables but keeps the new one at a higher priority
    /// which implies we look at this one more closely compared to the previous one
    pub fn merge_user_context(self, mut new_user_context: UserContext) -> Self {
        let variables_to_select = self
            .variables
            .into_iter()
            .filter(|already_present_variable| {
                // this is a negative filter, we do not want to repeate variables
                // which are the same file path over here
                !new_user_context.variables.iter().any(|new_variable| {
                    // if both the variables are files and they are the same file
                    // then dedup it over here, to the best of our ability
                    if new_variable.is_file() && already_present_variable.is_file() {
                        &new_variable.fs_file_path == &already_present_variable.fs_file_path
                    } else if new_variable.is_code_symbol()
                        && already_present_variable.is_code_symbol()
                    {
                        new_variable.name == already_present_variable.name
                    } else if new_variable.is_selection() && already_present_variable.is_selection()
                    {
                        &new_variable.content == &already_present_variable.content
                    } else {
                        false
                    }
                })
            })
            .collect::<Vec<_>>();
        new_user_context.variables.extend(variables_to_select);
        new_user_context
    }
}

#[async_recursion]
pub async fn read_folder_selection(
    folder_path: String,
    file_extension_filters: HashSet<String>,
) -> Result<String, UserContextError> {
    let mut output = String::new();
    output.push_str(&format!(
        "<selection_item>\n<folder>\n<name>\n{}\n</name>\n<file_content>\n",
        folder_path
    ));

    let mut entries = tokio::fs::read_dir(folder_path.to_owned())
        .await
        .map_err(|_| UserContextError::UnableToReadFromPath(folder_path.to_owned()))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| UserContextError::UnableToReadFromPath(folder_path.to_owned()))?
    {
        let path = entry.path();

        if path.is_file() {
            let file_path = path.to_str().unwrap().to_owned();
            let path_extension = path
                .extension()
                .map(|extension| extension.to_str())
                .flatten()
                .map(|extension| extension.to_owned())
                .unwrap_or_default();
            if path
                .extension()
                .map(|extension| extension == "json")
                .unwrap_or_default()
            {
                let content = tokio::fs::read_to_string(&path)
                    .await
                    .map_err(|_| UserContextError::UnableToReadFromPath(file_path.clone()));
                if let Ok(content) = content {
                    if content.lines().collect::<Vec<_>>().len() >= 50 {
                        // just grab the first 50 lines and push it to the contet
                        output.push_str(&format!(
                            "<file_content>\n<file_path>\n{}\n</file_path>\n<content>\n{}\nTruncated...\n</content>\n</file_content>\n",
                            file_path, content.lines().take(50).collect::<Vec<_>>().join("\n")
                        ));
                    } else {
                        output.push_str(&format!(
                            "<file_content>\n<file_path>\n{}\n</file_path>\n<content>\n{}</content>\n</file_content>\n",
                            file_path, content
                        ));
                    }
                }
                // if we are in the json flow, then we have already consumed
                // this file and should break
                continue;
            }
            // we also check the filter here to make sure we are including files
            // which are passing the filter if the filter is non-emtpy, otherwise
            // this block does not matter
            if !file_extension_filters.is_empty()
                && !file_extension_filters.contains(&path_extension)
            {
                continue;
            }
            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(|_| UserContextError::UnableToReadFromPath(file_path.clone()));
            if let Ok(content) = content {
                let content_type = guess_content(content.as_bytes());
                match content_type {
                    ProbableFileKind::Text(content) => {
                        output.push_str(&format!(
                            "<file_content>\n<file_path>\n{}\n</file_path>\n<content>\n{}</content>\n</file_content>\n",
                            file_path, content
                        ));
                    }
                    ProbableFileKind::Binary => {
                        output.push_str(&format!(
                            "<file_content>\n<file_path>\n{}\n</file_path>\n<content>\nBinary Blob</content>\n</file_content>\n",
                            file_path
                        ));
                    }
                }
            }
        } else if path.is_dir() {
            let sub_folder_output = read_folder_selection(
                path.to_str()
                    .expect("might not work on windows :(")
                    .to_owned(),
                file_extension_filters.clone(),
            )
            .await?;
            output.push_str(&sub_folder_output);
        }
    }

    output.push_str("</file_content>\n</folder>\n</selection_item>");
    Ok(output)
}
