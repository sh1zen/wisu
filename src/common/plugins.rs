use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

type FilterFn = Box<dyn Fn(Box<dyn Any + Send>) -> Box<dyn Any + Send> + Send + Sync>;

static FILTERS: Lazy<Mutex<HashMap<String, HashMap<TypeId, Vec<FilterFn>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn add_filter<T>(
    hook: impl Into<String>,
    filter: impl Fn(T) -> T + Send + Sync + 'static,
) where
    T: Any + Send + 'static,
{
    let hook = hook.into();
    let mut filters = FILTERS.lock().unwrap();

    filters
        .entry(hook)
        .or_default()
        .entry(TypeId::of::<T>())
        .or_default()
        .push(Box::new(move |val: Box<dyn Any + Send>| {
            let val = *val.downcast::<T>().unwrap();
            Box::new(filter(val)) as Box<dyn Any + Send>
        }));
}

pub fn apply_filter<T>(hook: &str, value: T) -> T
where
    T: Any + Send + 'static,
{
    let filters = FILTERS.lock().unwrap();
    let mut val: Box<dyn Any + Send> = Box::new(value);

    if let Some(hook_map) = filters.get(hook) {
        if let Some(filter_list) = hook_map.get(&TypeId::of::<T>()) {
            for filter in filter_list {
                val = filter(val);
            }
        }
    }

    *val.downcast::<T>().expect("Type mismatch in filter application")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_apply_filter() {
        // Hook: "increment", type: i32
        add_filter("increment", |x: i32| x + 1);
        add_filter("increment", |x: i32| x * 2);

        let result = apply_filter("increment", 3);
        // Step by step: (3 + 1) * 2 = 8
        assert_eq!(result, 8);

        // Hook: "greet", type: String
        add_filter("greet", |s: String| format!("Hello, {}!", s));
        add_filter("greet", |s: String| s.to_uppercase());

        let result_str = apply_filter("greet", "world".to_string());
        // Step by step: "Hello, world!" -> "HELLO, WORLD!"
        assert_eq!(result_str, "HELLO, WORLD!");

        // Test hook with no filters returns input unchanged
        let untouched = apply_filter("nonexistent", 42);
        assert_eq!(untouched, 42);
    }

    #[test]
    fn test_type_mismatch_panics() {
        // Adding an i32 filter
        add_filter("test", |x: i32| x + 10);
        // Applying it to should do nothing
        let c = apply_filter("test", "ciao");

        assert_eq!(c, "ciao");
    }
}
