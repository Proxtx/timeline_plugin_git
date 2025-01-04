use {
    client_api::{plugin::{PluginData, PluginEventData, PluginTrait}, result::EventResult, style::Style}, leptos::{view, IntoView, View}, serde::Deserialize
};

pub struct Plugin {
    
}

impl PluginTrait for Plugin {
    fn get_style(&self) -> Style {
        Style::Acc1
    }

    async fn new(_data: PluginData) -> Self
        where
            Self: Sized {
        Plugin {}
    }

    fn get_component(&self, data: PluginEventData) -> EventResult<Box<dyn FnOnce() -> leptos::View>> {
        let data = data.get_data::<DatabaseCommit>()?;
        Ok(Box::new(
            move || -> View {
                view! {
                    <div style="color: var(--lightColor); padding: calc(var(--contentSpacing) * 0.5); display: flex; flex-direction: column; width: 100%; gap: calc(var(--contentSpacing) * 0.5); background-color: var(--accentColor1);align-items: start; box-sizing: border-box;">
                        <h3>{move || { data.repository_name.clone() }}</h3>
                        <a>{move || { data.message.clone() }}</a>
                        <a>{move || { data.author.clone() }}</a>
                    </div>
                }.into_view()
            }
        ))
    }
}

#[derive(Debug, Deserialize, Clone)]
struct DatabaseCommit {
    message: String,
    author: String,
    repository_name: String,
}
