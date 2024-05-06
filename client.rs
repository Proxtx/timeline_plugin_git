use {
    leptos::{view, View, IntoView},
    serde::Deserialize
};

pub struct Plugin {
    
}

impl crate::plugin_manager::Plugin for Plugin {
    fn get_style(&self) -> crate::plugin_manager::Style {
        crate::plugin_manager::Style::Acc1
    }

    async fn new(_data: crate::plugin_manager::PluginData) -> Self
        where
            Self: Sized {
        Plugin {}
    }

    fn get_component(&self, data: crate::plugin_manager::PluginEventData) -> crate::event_manager::EventResult<Box<dyn FnOnce() -> leptos::View>> {
        let data = data.get_data::<DatabaseCommit>()?;
        Ok(Box::new(
            move || -> View {
                view! {
                    <div style="color: var(--lightColor); padding: calc(var(--contentSpacing) * 0.5); display: flex; flex-direction: column; width: 100%; gap: calc(var(--contentSpacing) * 0.5); background-color: var(--accentColor1);align-items: start;">
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
