//! Independent work spread over the machine's cores: fitting one section does
//! not read another, and a loft fits dozens of them many times over.

/// `items.iter().map(f)`, in order, on as many threads as there are cores —
/// on one where the platform has no threads.
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = if cfg!(target_family = "wasm") {
        1
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    };
    if threads < 2 || items.len() < 2 {
        return items.iter().map(f).collect();
    }
    let chunk = items.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let parts: Vec<_> = items
            .chunks(chunk)
            .map(|part| scope.spawn(|| part.iter().map(&f).collect::<Vec<R>>()))
            .collect();
        parts
            .into_iter()
            .flat_map(|part| part.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_the_order_of_its_items() {
        let items: Vec<usize> = (0..1000).collect();
        assert_eq!(super::map(&items, |i| i * 2), items.iter().map(|i| i * 2).collect::<Vec<_>>());
        assert_eq!(super::map(&[] as &[usize], |i| *i), Vec::<usize>::new());
    }
}
