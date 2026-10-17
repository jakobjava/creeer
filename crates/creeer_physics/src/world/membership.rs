//! Bodies leaving and joining a world: removed, kept, taken out to be put back unchanged
//! (simulation LOD parks bodies far from every player, F7.2, ADR-0019).

use super::{Body, World};

impl World {
    /// Removes the bodies `keep` refuses, the rest in their order, and returns each old
    /// index's new one. What the world kept of the last step is renumbered (friction
    /// anchors) or dropped (its contacts and groups, found again in the next step): left as
    /// they were, they would join other bodies than they did.
    pub fn retain_bodies(
        &mut self,
        mut keep: impl FnMut(usize, &Body) -> bool,
    ) -> Vec<Option<usize>> {
        let mut next = 0;
        let map: Vec<Option<usize>> = self
            .bodies
            .iter()
            .enumerate()
            .map(|(i, body)| {
                keep(i, body).then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        let mut index = 0;
        self.bodies.retain(|_| {
            index += 1;
            map[index - 1].is_some()
        });
        self.renumber(&map);
        map
    }

    /// Takes body `index` out of the world (the bodies after it move down by one), as
    /// [`Self::retain_bodies`] does.
    pub fn remove_body(&mut self, index: usize) -> Body {
        let body = self.bodies.remove(index);
        let map: Vec<Option<usize>> = (0..=self.bodies.len())
            .map(|i| match i.cmp(&index) {
                std::cmp::Ordering::Less => Some(i),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some(i - 1),
            })
            .collect();
        self.renumber(&map);
        body
    }

    fn renumber(&mut self, map: &[Option<usize>]) {
        self.left = self.left.renumbered(map);
        self.anchors = self
            .anchors
            .iter()
            .filter_map(|a| a.renumbered(map))
            .collect();
        self.contacts.clear();
        self.groups.clear();
    }

    /// Takes out the bodies `pick` chooses, in their order, and returns them with each old
    /// index's new one (as [`Self::retain_bodies`]). They can be put back unchanged
    /// ([`Self::put_back`]).
    pub fn take_bodies(
        &mut self,
        mut pick: impl FnMut(usize, &Body) -> bool,
    ) -> (Vec<Body>, Vec<Option<usize>>) {
        let picked: Vec<bool> = (self.bodies.iter().enumerate())
            .map(|(i, b)| pick(i, b))
            .collect();
        let mut taken = Vec::new();
        let mut kept = Vec::with_capacity(self.bodies.len());
        for (body, &out) in std::mem::take(&mut self.bodies).into_iter().zip(&picked) {
            if out {
                taken.push(body);
            } else {
                kept.push(body);
            }
        }
        self.bodies = kept;
        let mut next = 0;
        let map: Vec<Option<usize>> = picked
            .iter()
            .map(|&out| {
                (!out).then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        self.renumber(&map);
        (taken, map)
    }

    /// Puts back a body taken out of this world, exactly as it was (unlike [`Self::add`],
    /// which fits a new body to the world: its planet's gravity, its rest place, its
    /// solver's history). Returns its index.
    pub fn put_back(&mut self, body: Body) -> usize {
        self.bodies.push(body);
        self.bodies.len() - 1
    }
}
