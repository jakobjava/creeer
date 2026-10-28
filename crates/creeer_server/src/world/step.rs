//! The world's step (`ServerWorld::step`): machines, surroundings, people and carrying about
//! the physics' step, then what follows from it: blows, fastenings (G2.3), trees and ageing.

use super::*;

impl ServerWorld {
    /// One step.
    pub fn step(&mut self) {
        let scope = profile::scope("machines");
        self.tick_machines();
        drop(scope);
        let scope = profile::scope("surroundings");
        self.surroundings.push(&mut self.world);
        drop(scope);
        self.people_before();
        self.carry_before();
        self.world.step();
        // (What the legs pushed with, for the people's injuries: before their struts come off.)
        let pushes = self.world.strut_pushes();
        character::release(&mut self.world, &mut self.reactions);
        self.carry_after();
        let scope = profile::scope("people");
        self.people_after(&pushes);
        drop(scope);
        let scope = profile::scope("surroundings");
        self.surroundings.settle(&mut self.world, DT);
        drop(scope);
        self.tick += 1;
        let scope = profile::scope("history");
        self.remember();
        drop(scope);
        self.blows_fall();
        self.fastenings_due();
        self.tell_grips();
        self.come_apart();
        self.lay_resting();
        let scope = profile::scope("trees");
        self.trees_after_step();
        drop(scope);
        let _scope = profile::scope("ageing");
        self.age_after_step();
    }
}
