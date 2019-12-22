pub mod prelude;

mod iter {
    use crate::flow::{FlowEnum, FlowID};
    pub enum Iter<'a, F: From<&'a FlowEnum>, T: Clone> {
        FlowTable {
            ptr: usize,
            cur_list: usize,
            id_lists: Vec<&'a Vec<Option<FlowID>>>,
            flow_list: &'a Vec<Option<FlowEnum>>,
            infos: &'a Vec<Option<T>>,
            _marker: std::marker::PhantomData<&'a F>,
        },
        DiffTable {
            ptr: usize,
            cur_list: usize,
            id_lists: Vec<&'a Vec<FlowID>>,
            flow_list: &'a Vec<Option<FlowEnum>>,
            infos: &'a Vec<Option<T>>,
            _marker: std::marker::PhantomData<&'a F>,
        },
    }

    impl<'a, F: From<&'a FlowEnum>, T: Clone> Iterator for Iter<'a, F, T> {
        type Item = (F, &'a T);
        fn next(&mut self) -> Option<(F, &'a T)> {
            match self {
                Iter::FlowTable {
                    ref mut ptr,
                    ref mut cur_list,
                    flow_list,
                    id_lists,
                    infos,
                    ..
                } => {
                    while *cur_list < id_lists.len() {
                        let id_list = id_lists[*cur_list];
                        while *ptr < id_list.len() {
                            let cur_ptr = *ptr;
                            *ptr += 1;
                            if let Some(id) = id_list[cur_ptr] {
                                let flow = flow_list[id.0].as_ref().unwrap();
                                return Some((flow.into(), infos[id.0].as_ref().unwrap()));
                            }
                        }
                        *cur_list += 1;
                    }
                }
                Iter::DiffTable {
                    ref mut ptr,
                    ref mut cur_list,
                    flow_list,
                    id_lists,
                    infos,
                    ..
                } => {
                    while *cur_list < id_lists.len() {
                        let id_list = id_lists[*cur_list];
                        if *ptr < id_list.len() {
                            let id = id_list[*ptr];
                            *ptr += 1;
                            let flow = flow_list[id.0].as_ref().unwrap();
                            return Some((flow.into(), infos[id.0].as_ref().unwrap()));
                        }
                        *cur_list += 1;
                    }
                }
            }
            None
        }
    }

    pub struct IterMut<'a, F: From<&'a FlowEnum>, T: Clone> {
        pub(super) iter: Iter<'a, F, T>,
    }

    impl<'a, F: From<&'a FlowEnum>, T: Clone> Iterator for IterMut<'a, F, T> {
        type Item = (F, &'a mut T);
        fn next(&mut self) -> Option<(F, &'a mut T)> {
            self.iter
                .next()
                .map(|(flow, t)| unsafe { (flow, &mut *(t as *const T as *mut T)) })
        }
    }
}
