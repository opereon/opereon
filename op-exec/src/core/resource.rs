use std::any::TypeId;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard};

use uuid::Uuid;

pub trait Resource: std::fmt::Debug + 'static {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl dyn Resource {
    pub fn downcast_ref<T: Resource>(&self) -> Option<&T> {
        if self.type_id() == TypeId::of::<T>() {
            unsafe { Some(&*(self as *const dyn Resource as *const T)) }
        } else {
            None
        }
    }

    pub fn downcast_mut<T: Resource>(&mut self) -> Option<&mut T> {
        if self.type_id() == TypeId::of::<T>() {
            unsafe { Some(&mut *(self as *mut dyn Resource as *mut T)) }
        } else {
            None
        }
    }
}

pub struct ResourceRef(Arc<Mutex<Box<dyn Resource>>>);

impl ResourceRef {
    pub fn lock<R: Resource>(&self) -> Lock<R> {
        let mut guard = self.0.lock().unwrap();
        let value = guard
            .deref_mut()
            .downcast_mut::<R>()
            .expect("resource has wrong type") as *mut R;
        Lock { guard, value }
    }
}

#[allow(dead_code)]
pub struct Lock<'a, R: Resource> {
    guard: MutexGuard<'a, Box<dyn Resource>>,
    value: *mut R,
}

impl<'a, R: Resource> Deref for Lock<'a, R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.value }
    }
}

impl<'a, R: Resource> DerefMut for Lock<'a, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.value }
    }
}

#[derive(Debug)]
pub struct ResourceManager {
    resources: HashMap<Uuid, Arc<Mutex<Box<dyn Resource>>>>,
}

impl ResourceManager {
    pub fn new() -> ResourceManager {
        ResourceManager {
            resources: HashMap::new(),
        }
    }

    pub fn get(&self, id: Uuid) -> ResourceRef {
        ResourceRef(self.resources.get(&id).expect("resource not found").clone())
    }

    pub fn put(&mut self, id: Uuid, resource: Box<dyn Resource>) {
        self.resources
            .insert(id, Arc::new(Mutex::new(resource as Box<dyn Resource>)));
    }

    pub fn remove(&mut self, id: Uuid) {
        self.resources.remove(&id);
    }
}

unsafe impl Send for ResourceManager {}

unsafe impl Sync for ResourceManager {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Res1 {
        prop: String,
    }

    impl Resource for Res1 {}

    #[derive(Debug)]
    struct Res2 {
        prop: usize,
    }

    impl Resource for Res2 {}

    #[test]
    fn get_resource_should_work_for_correct_id() {
        let mut m = ResourceManager::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        m.put(
            id1,
            box Res1 {
                prop: "string".into(),
            },
        );
        m.put(id2, box Res2 { prop: 12 });

        let r = m.get(id1);
        let r: Lock<Res1> = r.lock();
        assert_eq!(r.prop, "string");

        let r = m.get(id2);
        let r: Lock<Res2> = r.lock();
        assert_eq!(r.prop, 12);
    }

    #[test]
    #[should_panic(expected = "resource not found")]
    fn get_resource_should_fail_for_wrong_id() {
        let mut m = ResourceManager::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        m.put(
            id1,
            box Res1 {
                prop: "string".into(),
            },
        );

        let r = m.get(id2);
        let _: Lock<Res1> = r.lock();
    }

    #[test]
    #[should_panic(expected = "resource has wrong type")]
    fn get_resource_should_fail_for_wrong_type() {
        let mut m = ResourceManager::new();
        let id1 = Uuid::new_v4();

        m.put(
            id1,
            box Res1 {
                prop: "string".into(),
            },
        );

        let r = m.get(id1);
        let _: Lock<Res2> = r.lock();
    }
}
