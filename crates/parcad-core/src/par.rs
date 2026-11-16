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

/// [`map`] for items of uneven cost, each thread taking the next one as it
/// finishes and working in a state of its own from `init`.
pub fn map_with<T: Sync, S, R: Send>(items: &[T], init: impl Fn() -> S + Sync, f: impl Fn(&mut S, &T) -> R + Sync) -> Vec<R> {
    let threads = if cfg!(target_family = "wasm") {
        1
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get()).min(items.len())
    };
    if threads < 2 {
        let mut state = init();
        return items.iter().map(|item| f(&mut state, item)).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut state = init();
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(i) else { break mine };
                        mine.push((i, f(&mut state, item)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)))
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn uneven_items_come_back_in_order_each_done_once() {
        let items: Vec<usize> = (0..500).collect();
        let doubled = super::map_with(&items, || 0usize, |seen, i| {
            *seen += 1;
            i * 2
        });
        assert_eq!(doubled, items.iter().map(|i| i * 2).collect::<Vec<_>>());
        assert_eq!(super::map_with(&[] as &[usize], || (), |_, i| *i), Vec::<usize>::new());
    }

    #[test]
    fn keeps_the_order_of_its_items() {
        let items: Vec<usize> = (0..1000).collect();
        assert_eq!(super::map(&items, |i| i * 2), items.iter().map(|i| i * 2).collect::<Vec<_>>());
        assert_eq!(super::map(&[] as &[usize], |i| *i), Vec::<usize>::new());
    }
}
