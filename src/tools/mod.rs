pub mod breaking;
pub mod doomsday;
pub mod encyclopedia;
pub mod fetch;
pub mod news;
pub mod pizza;
pub mod predictions;
pub mod stocks;
pub mod web;

pub fn interleave<T>(sources: Vec<Vec<T>>, limit: usize) -> Vec<T> {
    let mut output = Vec::with_capacity(limit);
    let mut iterators: Vec<_> = sources.into_iter().map(IntoIterator::into_iter).collect();
    loop {
        let mut advanced = false;
        for iterator in iterators.iter_mut() {
            if let Some(item) = iterator.next() {
                output.push(item);
                advanced = true;
                if output.len() >= limit {
                    return output;
                }
            }
        }
        if !advanced {
            break;
        }
    }
    output
}
