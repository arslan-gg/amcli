use amcli_view::layout::{Algorithm, Item, place};
fn items(n: usize) -> Vec<Item> {
    (0..n).map(|i| Item { id: format!("id{i}"), name: format!("N{i}"), w: 120, h: 55 }).collect()
}
#[test]
fn crowded() {
    let it = items(8);
    let mut edges = vec![(0, 1), (1, 7), (0, 7)];
    for i in 2..7 {
        edges.push((0, i));
        edges.push((i, 7));
    }
    let p = place(&it, &edges, Algorithm::Sugiyama);
    for (i, r) in p.rects.iter().enumerate() {
        println!("  {i}: x={} y={} w={}", r.x, r.y, r.w);
    }
}
