use std::rc::Rc;
use crate::utils::stream::{TSN, AVB};
use crate::network::MemorizingGraph;
use crate::network::Network;
use crate::component::flowtable::FlowTable;
use crate::component::GCL;
use super::cost::{RoutingCost, Calculator};
use crate::scheduler::schedule_online;

type Route = Vec<usize>;

/// 這個結構預期會被複製很多次，因此其中的每個元件都應儘可能想辦法降低複製成本
#[derive(Clone)]
pub struct NetworkWrapper {
    pub flow_table: FlowTable,
    pub old_new_table: Option<Rc<FlowTable>>, // 在每次運算中類似常數，故用 RC 來包
    pub get_route_func: Rc<dyn Fn(usize, usize, usize) -> *const Route>,
    pub gcl: GCL,
    pub graph: MemorizingGraph,
    pub tsn_fail: bool,
}

impl NetworkWrapper {
    pub fn new<F>(graph: Network, get_route_func: F) -> Self
    where
        F: 'static + Fn(usize, usize, usize) -> *const Route,
    {
        NetworkWrapper {
            flow_table: FlowTable::new(),
            old_new_table: None,
            gcl: GCL::new(1),
            tsn_fail: false,
            graph: MemorizingGraph::new(graph),
            get_route_func: Rc::new(get_route_func),
        }
    }
    /// 插入新的資料流，同時會捨棄先前的新舊表，並創建另一份新舊表
    pub fn insert(&mut self, tsns: Vec<TSN>, avbs: Vec<AVB>, default_info: usize) {
        // 釋放舊的表備份表
        self.old_new_table = None;
        // 插入
        let new_ids = self.flow_table.insert(tsns, avbs, default_info.clone());
        let mut reconf = self.flow_table.clone_as_diff();

        for &flow_id in new_ids.iter() {
            reconf.update_info_force_diff(flow_id, default_info.clone());
        }

        self.update_avb(&reconf);
        self.update_tsn(&reconf);

        let mut old_new_table = self.flow_table.clone();
        old_new_table.insert_xxx(new_ids);
        self.old_new_table = Some(Rc::new(old_new_table));
    }
    pub fn get_route(&self, flow_id: usize) -> &Route {
        let (src, dst) = self.flow_table.ends(flow_id);
        let info = self.flow_table.get_info(flow_id).unwrap();
        let route = (self.get_route_func)(src, dst, info);
        unsafe { &*route }
    }
    pub fn get_old_route(&self, flow_id: usize) -> Option<usize> {
        if let Some(t) = self
            .old_new_table
            .as_ref()
            .unwrap()
            .get_info(flow_id)
        {
            Some(t)
        } else {
            None
        }
    }
    pub fn update_single_avb(&mut self, id: usize, info: usize) {
        // NOTE: 因為 self.graph 與 self.get_route 是平行所有權
        let graph = unsafe { &mut (*(self as *mut Self)).graph };
        let og_route = self.get_route(id);
        // 忘掉舊的
        graph.update_flowid_on_route(false, id, og_route);
        self.flow_table.update_info(id, info);
        let new_route = self.get_route(id);
        // 記憶新的
        graph.update_flowid_on_route(true, id, new_route);
    }
    /// 更新 AVB 資料流表與圖上資訊
    pub fn update_avb(&mut self, diff: &FlowTable) {
        for &id in diff.iter_avb_diff() {
            let info = diff.get_info(id).unwrap();
            self.update_single_avb(id, info.clone());
        }
    }
    /// 更新 TSN 資料流表與 GCL
    pub fn update_tsn(&mut self, diff: &FlowTable) {
        // NOTE: 在 schedule_online 函式中就會更新資料流表（這當然是個不太好的實作……）
        //       因此在這裡就不用執行 self.flow_table.update_info()
        for &id in diff.iter_tsn_diff() {
            // NOTE: 拔除 GCL
            let route = self.get_route(id);
            let links = self
                .graph
                .get_links_id_bandwidth(route)
                .iter()
                .map(|(ends, _)| *ends)
                .collect();
            self.gcl.delete_flow(&links, id);
        }
        let _self = self as *const Self;
        let result = schedule_online(&mut self.flow_table, diff, &mut self.gcl, |id, k| {
            // NOTE: 因為 self.flow_table.get 和 self.get_route_func 和 self.graph 與其它部份是平行所有權
            unsafe {
                let (src, dst) = (*_self).flow_table.ends(id);
                let route = &*(((*_self).get_route_func)(src, dst, k));
                (*_self).graph.get_links_id_bandwidth(route)
            }
        });
        if result.is_err() {
            self.tsn_fail = true;
        } else {
            // TODO: 應該如何處理 result = Ok(bool) ？
            self.tsn_fail = false;
        }
    }
    pub fn get_flow_table(&self) -> &FlowTable {
        &self.flow_table
    }
    /// 路徑為可選參數，若不給代表照資料流表來走
    pub fn compute_avb_wcd(&self, flow: usize, route: Option<usize>) -> u32 {
        self._compute_avb_wcd(flow, route)
    }
    pub fn compute_all_cost(&self) -> RoutingCost {
        self._compute_all_cost()
    }
    pub fn compute_single_avb_cost(&self, flow: usize) -> RoutingCost {
        self._compute_single_avb_cost(flow)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::flow::{data::TSNData, TSNFlow};
    use crate::read_topo_from_file;
    use std::collections::HashMap;

    struct Env(HashMap<(usize, usize), Vec<Route>>);
    impl Env {
        pub fn new() -> Self {
            let mut map = HashMap::new();
            map.insert((0, 4), vec![vec![0, 4], vec![0, 5, 4]]);
            map.insert((1, 2), vec![vec![1, 0, 4, 2]]);
            Env(map)
        }
        pub fn get_route(&self, src: usize, dst: usize, i: usize) -> *const Route {
            &self.0.get(&(src, dst)).unwrap()[i]
        }
    }

    fn init() -> (NetworkWrapper<usize>, Vec<TSNFlow>) {
        let graph = read_topo_from_file("test_graph.json");
        let env = Env::new();
        let wrapper = NetworkWrapper::new(graph, move |flow, k: &usize| match flow {
            FlowEnum::AVB(flow) => env.get_route(flow.src, flow.dst, *k),
            FlowEnum::TSN(flow) => env.get_route(flow.src, flow.dst, *k),
        });
        let flows = vec![
            TSNFlow {
                id: 0.into(),
                src: 0,
                dst: 4,
                size: 100,
                period: 100,
                max_delay: 100,
                spec_data: TSNData { offset: 0 },
            },
            TSNFlow {
                id: 0.into(),
                src: 0,
                dst: 4,
                size: 100,
                period: 150,
                max_delay: 150,
                spec_data: TSNData { offset: 0 },
            },
            TSNFlow {
                id: 0.into(),
                src: 1,
                dst: 2,
                size: 100,
                period: 200,
                max_delay: 200,
                spec_data: TSNData { offset: 0 },
            },
        ];
        (wrapper, flows)
    }

    #[test]
    fn test_insert_get_route() {
        let (mut wrapper, flows) = init();
        wrapper.insert(flows.clone(), vec![], 0);

        wrapper.flow_table.update_info(1.into(), 1);

        assert_eq!(&vec![0, 4], wrapper.get_route(0.into()));
        assert_eq!(&vec![0, 5, 4], wrapper.get_route(1.into()));
        assert_eq!(&vec![1, 0, 4, 2], wrapper.get_route(2.into()));
        let old_new = wrapper
            .old_new_table
            .as_ref()
            .unwrap()
            .get_info(1.into())
            .unwrap();
        assert_eq!(&OldNew::New, old_new);

        wrapper.insert(flows.clone(), vec![], 0);
        assert_eq!(&vec![0, 4], wrapper.get_route(3.into()));
        assert_eq!(&vec![0, 4], wrapper.get_route(4.into()));
        assert_eq!(&vec![1, 0, 4, 2], wrapper.get_route(5.into()));
        let old_new = wrapper
            .old_new_table
            .as_ref()
            .unwrap()
            .get_info(1.into())
            .unwrap();
        assert_eq!(&OldNew::Old(1), old_new);
        let old_new = wrapper
            .old_new_table
            .as_ref()
            .unwrap()
            .get_info(3.into())
            .unwrap();
        assert_eq!(&OldNew::New, old_new);
    }
    #[test]
    #[should_panic]
    fn test_clone_and_insert_should_panic() {
        let (mut wrapper, flows) = init();
        wrapper.insert(flows.clone(), vec![], 0);
        let mut wrapper2 = wrapper.clone();
        wrapper2.insert(flows.clone(), vec![], 0);
    }
    #[test]
    fn test_clone() {
        let (mut wrapper, flows) = init();
        wrapper.insert(flows.clone(), vec![], 0);
        let wrapper2 = wrapper.clone();
        wrapper.flow_table.update_info(0.into(), 99);
        assert_eq!(&99, wrapper.flow_table.get_info(0.into()).unwrap());
        assert_eq!(&0, wrapper2.flow_table.get_info(0.into()).unwrap());
    }
}
