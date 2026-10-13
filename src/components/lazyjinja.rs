use std::sync::Arc;

use minijinja::{Environment, Value};

use crate::components::{tpl_helpers, vite_assets::ViteAssets};

/// A wrapper around a [minijinja::Environment]. This is meant to be added to
/// [crate::components::app_state::AppState] inside an [Arc], so it's available
/// for requests. Each request should then call [Self::acquire_env] to acquire
/// its personalized copy of the minijinja Environment.
///
/// Calling this will always give you a clone of an internal base environment.
/// This might sound like an odd thing to do, but it has a purpose.
///
/// In Development, the [minijinja::path_loader] caches the compiled templates
/// in the environment itself. So if we were to re-use that for all requests,
/// we'd have to restart the app to apply template changes. If we throw away the
/// Environment after every request, we get magical live template reloading,
/// which just is a nice bit of DevEx.
///
/// In Release builds, this uses a mechanism that is _similar_ to the one
/// provided by `minijinja_embed`, but not quite. The upstream implementation
/// immediately calls `env.add_template` on every template, and then stores the
/// compiled results in the Environment. This is nice for for pure performance,
/// as each template only needs to be compiled once, but there's a not
/// insignificant memory cost to each template, that will just stick around
/// forever. This implementation also does statically embed the template strings
/// into the binary, but it doesn't `add_template` them, it just stores the
/// template strings in a `HashMap<&'static str, &'static str>`. A custom loader
/// is used to then compile those for each environment. This is, technically,
/// less efficient - but template compilation is not a real cost. In a local
/// benchmark with a fairly typical template (layout with two blocks, a partial,
/// and a few simple ifs), it added around 50µs to each response. If I ever get
/// into a point where the difference matters, this is easily reverted for that
/// project. Most of the time, I don't care.
#[derive(Default, Debug)]
pub struct LazyJinja(minijinja::Environment<'static>);

impl LazyJinja {
    pub fn new() -> Self {
        let mut base_env = Environment::new();
        minijinja_contrib::add_to_environment(&mut base_env);

        #[cfg(debug_assertions)]
        base_env.set_loader(minijinja::path_loader("templates"));

        #[cfg(not(debug_assertions))]
        {
            let static_tpls = include!(concat!(env!("OUT_DIR"), "/lazyjinja_templates.rs"));
            base_env.set_loader(move |name| match static_tpls.get(name) {
                Some(tpl) => Ok(Some((*tpl).to_string())),
                None => Ok(None::<String>),
            });
        }

        let vite_assets = Arc::new(ViteAssets::new());

        let vite_assets_clone = vite_assets.clone();
        base_env.add_function("vite_asset", move |entry: &str| {
            Ok(Value::from(vite_assets_clone.get_asset_url(entry)))
        });

        let vite_assets_clone = vite_assets.clone();
        base_env.add_function("vite_css", move |entry: &str| {
            Ok(Value::from(vite_assets_clone.get_css_urls(entry)))
        });

        let vite_assets_clone = vite_assets.clone();
        base_env.add_function("vite_client", move || {
            Ok(Value::from(vite_assets_clone.get_vite_client()))
        });

        base_env.add_filter("formatted_level_id", tpl_helpers::formatted_level_id);
        base_env.add_filter("ms_to_minsecs", tpl_helpers::ms_to_minsecs);
        base_env.add_filter("tag_list", tpl_helpers::tag_list);
        base_env.add_filter("tag_name", tpl_helpers::tag_name);
        base_env.add_function("clear_condition_text", tpl_helpers::clear_condition_text);

        Self(base_env)
    }

    pub fn acquire_env(&self) -> Environment<'static> {
        self.0.clone()
    }
}
