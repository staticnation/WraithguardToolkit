//! The vertex fetch order: the vertices renumbered in the order the triangles first use
//! them, so the GPU reads the vertex buffer front to back. Run after the triangle order
//! (`triorder`), since it follows the triangles. From meshoptimizer 1.3's
//! `vfetchoptimizer.cpp` and, for the remaps, `indexgenerator.cpp`.
//!
//! A remap table holds, for each old vertex, its new number, or `u32::MAX` (the C++'s
//! `~0u`) for a vertex no triangle uses; those are dropped.

/// `meshopt_optimizeVertexFetchRemap`: the remap table for the fetch order. Returns the
/// table (one entry per vertex) and the number of vertices kept.
///
/// Every index must be below `vertex_count` (the C++ asserts it).
pub fn optimize_vertex_fetch_remap(indices: &[u32], vertex_count: usize) -> (Vec<u32>, usize) {
    let mut destination = vec![u32::MAX; vertex_count];

    let mut next_vertex: u32 = 0;

    for &index in indices {
        debug_assert!((index as usize) < vertex_count);

        if destination[index as usize] == u32::MAX {
            destination[index as usize] = next_vertex;
            next_vertex += 1;
        }
    }

    debug_assert!(next_vertex as usize <= vertex_count);

    (destination, next_vertex as usize)
}

/// `meshopt_optimizeVertexFetch`: the remap and the vertex copy in one go, for vertices
/// stored interleaved (`vertex_size` bytes each). Rewrites `indices` in place and returns
/// the new vertex buffer, cut to the vertices kept.
pub fn optimize_vertex_fetch(indices: &mut [u32], vertices: &[u8], vertex_count: usize, vertex_size: usize) -> Vec<u8> {
    assert!(vertex_size > 0 && vertex_size <= 256);
    debug_assert!(vertices.len() >= vertex_count * vertex_size);

    let mut destination = vec![0u8; vertex_count * vertex_size];

    // build vertex remap table
    let mut vertex_remap = vec![u32::MAX; vertex_count];

    let mut next_vertex: u32 = 0;

    for index in indices.iter_mut() {
        let i = *index as usize;
        debug_assert!(i < vertex_count);

        let remap = &mut vertex_remap[i];

        if *remap == u32::MAX {
            // vertex was not added to destination VB
            // add vertex
            let d = next_vertex as usize * vertex_size;
            destination[d..d + vertex_size].copy_from_slice(&vertices[i * vertex_size..(i + 1) * vertex_size]);

            *remap = next_vertex;
            next_vertex += 1;
        }

        // modify indices in place
        *index = *remap;
    }

    debug_assert!(next_vertex as usize <= vertex_count);

    destination.truncate(next_vertex as usize * vertex_size);
    destination
}

/// `meshopt_remapIndexBuffer`: the index list renumbered through `remap`. `indices` of
/// `None` is the C++'s null (an unindexed list: index `i` is vertex `i`), `index_count`
/// long.
pub fn remap_index_buffer(indices: Option<&[u32]>, index_count: usize, remap: &[u32]) -> Vec<u32> {
    let mut destination = vec![0u32; index_count];

    for i in 0..index_count {
        let index = match indices {
            Some(ix) => ix[i],
            None => i as u32,
        };
        debug_assert!(remap[index as usize] != u32::MAX);

        destination[i] = remap[index as usize];
    }

    destination
}

/// `meshopt_remapVertexBuffer`, over any per-vertex array: one item per vertex (a
/// position, a normal, a UV pair, a colour, a skin weight list...), moved to its new
/// number. Vertices the remap drops are left out; the result is `kept` long, `kept` being
/// what [`optimize_vertex_fetch_remap`] returned.
///
/// Generic rather than the C++'s byte blocks: a mesh's arrays are separate here, each of
/// its own type, and all of them must move with the same table.
pub fn remap_vertex_buffer<T: Clone + Default>(vertices: &[T], kept: usize, remap: &[u32]) -> Vec<T> {
    let mut destination = vec![T::default(); kept];

    for i in 0..vertices.len().min(remap.len()) {
        if remap[i] != u32::MAX {
            debug_assert!((remap[i] as usize) < kept);
            destination[remap[i] as usize] = vertices[i].clone();
        }
    }

    destination
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_use_numbers_the_vertex() {
        let (remap, kept) = optimize_vertex_fetch_remap(&[2, 0, 3, 3, 0, 2], 5);
        assert_eq!(kept, 3);
        assert_eq!(remap, vec![1, u32::MAX, 0, 2, u32::MAX]);
        assert_eq!(remap_index_buffer(Some(&[2, 0, 3, 3, 0, 2]), 6, &remap), vec![0, 1, 2, 2, 1, 0]);
        assert_eq!(remap_vertex_buffer(&['a', 'b', 'c', 'd', 'e'], kept, &remap), vec!['c', 'a', 'd']);
    }

    #[test]
    fn the_interleaved_form_agrees_with_the_remap() {
        let mut idx = vec![2u32, 0, 3];
        let vb: Vec<u8> = (0..20).collect(); // five vertices of four bytes
        let out = optimize_vertex_fetch(&mut idx, &vb, 5, 4);
        assert_eq!(idx, vec![0, 1, 2]);
        assert_eq!(out, vec![8, 9, 10, 11, 0, 1, 2, 3, 12, 13, 14, 15]);
    }

    #[test]
    fn an_unindexed_list_is_its_own_order() {
        assert_eq!(remap_index_buffer(None, 3, &[2, 0, 1]), vec![2, 0, 1]);
    }
}
