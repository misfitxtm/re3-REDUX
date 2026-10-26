//! `CPathFind` and `CPedPath`: the road and pavement graph, searches over it, and spawn queries.
use crate::collision::dist_to_line;
use crate::general::Rng;
use crate::math::{heading, normalise_2d, Matrix};
use glam::{Vec2, Vec3};

pub const NUM_PATHNODES: usize = 4930;
pub const NUM_CARPATHLINKS: usize = 2076;
pub const NUM_MAPOBJECTS: usize = 1250;
pub const NUM_PATHCONNECTIONS: usize = 10260;
pub const PATHNODESIZE: usize = 4500;
const NUMTEMPNODES: usize = 4000;
const MAX_DIST: i16 = i16::MAX - 1;
const MIN_PED_ROUTE_DISTANCE: f32 = 23.8;

pub const NODE_EXTERN: i8 = 1;
pub const NODE_INTERN: i8 = 2;
pub const USE_IN_ROADBLOCK: u8 = 1;
pub const OBJECT_EAST_WEST: u8 = 2;

pub const PATH_CAR: usize = 0;
pub const PATH_PED: usize = 1;

pub const SWITCH_OFF: u8 = 0;
pub const SWITCH_ON: u8 = 1;

pub const ROUTE_ADD_BLOCKADE: i8 = 0;
pub const ROUTE_NO_BLOCKADE: i8 = 1;

/// A solid thing a ped route has to walk around (`CPedPath::AddBlockade`'s input).
#[derive(Debug, Clone, Copy)]
pub struct Blockade {
    pub pos: Vec3,
    pub right: Vec3,
    pub forward: Vec3,
    pub bb_min: Vec3,
    pub bb_max: Vec3,
    pub bound_centre: Vec3,
    pub bound_radius: f32,
}

#[derive(Clone, Copy)]
struct PedPathNode {
    blockade: bool,
    x: u8,
    y: u8,
    id: i16,
    prev: i32,
    next: i32,
}

const GRID: usize = 40;
const GRID_NODES: usize = GRID * GRID;
const LIST_LEN: usize = 416;

/// `CPedPath::CalcPedRoute`: a grid search around the straight line between the two points.
/// Returns the route's points or `None`; `max_points` caps the count. The blockades are those near the route.
pub fn calc_ped_route(path_type: i8, position: Vec3, destination: Vec3, max_points: usize, blockades: &[Blockade]) -> Option<Vec<Vec3>> {
    let d = destination - position;
    if d.x.abs() > MIN_PED_ROUTE_DISTANCE || d.y.abs() > MIN_PED_ROUTE_DISTANCE || d.z.abs() > MIN_PED_ROUTE_DISTANCE {
        return None;
    }
    let mid = (position + destination) * 0.5;
    let start = Vec3::new(mid.x - 14.0, mid.y - 14.0, mid.z);
    let node_start_x = ((position.x - start.x) / 0.7) as i16;
    let node_start_y = ((position.y - start.y) / 0.7) as i16;
    let node_end_x = ((destination.x - start.x) / 0.7) as i16;
    let node_end_y = ((destination.y - start.y) / 0.7) as i16;
    if node_start_x == node_end_x && node_start_y == node_end_y {
        return None;
    }
    let mut nodes = vec![PedPathNode { blockade: false, x: 0, y: 0, id: i16::MAX, prev: -1, next: -1 }; GRID_NODES + LIST_LEN];
    for x in 0..GRID {
        for y in 0..GRID {
            let n = &mut nodes[x * GRID + y];
            n.x = x as u8;
            n.y = y as u8;
        }
    }
    if path_type != ROUTE_NO_BLOCKADE {
        for b in blockades {
            add_blockade(b, &mut nodes, start);
        }
    }
    for i in 0..LIST_LEN {
        nodes[GRID_NODES + i].prev = -1;
        nodes[GRID_NODES + i].next = -1;
    }
    let idx = |x: i16, y: i16| x as usize * GRID + y as usize;
    let start_node = idx(node_start_x, node_start_y);
    let end_node = idx(node_end_x, node_end_y);
    nodes[end_node].blockade = false;
    nodes[end_node].id = 0;
    nodes[end_node].prev = -1;
    nodes[end_node].next = GRID_NODES as i32;
    nodes[GRID_NODES].prev = end_node as i32;

    let add = |nodes: &mut Vec<PedPathNode>, n: usize, id: i16| {
        if !nodes[n].blockade && id < nodes[n].id {
            if nodes[n].id != i16::MAX {
                let (p, nx) = (nodes[n].prev, nodes[n].next);
                nodes[nx as usize].prev = p;
                if p >= 0 {
                    nodes[p as usize].next = nx;
                }
            }
            let slot = GRID_NODES + id as usize;
            let lp = nodes[slot].prev;
            nodes[n].prev = lp;
            nodes[n].next = slot as i32;
            if lp >= 0 {
                nodes[lp as usize].next = n as i32;
            }
            nodes[slot].prev = n as i32;
            nodes[n].id = id;
        }
    };

    let mut path_node_index = 0;
    while path_node_index < 414 {
        let mut prev = nodes[GRID_NODES + path_node_index].prev;
        while prev >= 0 && prev as usize != start_node {
            let (nx, ny) = (nodes[prev as usize].x as i16, nodes[prev as usize].y as i16);
            let base = path_node_index as i16;
            if nx > 0 {
                add(&mut nodes, idx(nx - 1, ny), base + 5);
                if ny > 0 {
                    add(&mut nodes, idx(nx - 1, ny - 1), base + 7);
                }
                if ny < 39 {
                    add(&mut nodes, idx(nx - 1, ny + 1), base + 7);
                }
            }
            if nx < 39 {
                add(&mut nodes, idx(nx + 1, ny), base + 5);
                if ny > 0 {
                    add(&mut nodes, idx(nx + 1, ny - 1), base + 7);
                }
                if ny < 39 {
                    add(&mut nodes, idx(nx + 1, ny + 1), base + 7);
                }
            }
            if ny > 0 {
                add(&mut nodes, idx(nx, ny - 1), base + 5);
            }
            if ny < 39 {
                add(&mut nodes, idx(nx, ny + 1), base + 5);
            }
            prev = nodes[prev as usize].prev;
            if prev < 0 {
                break;
            }
        }
        if prev >= 0 && prev as usize == start_node {
            break;
        }
        path_node_index += 1;
    }
    if path_node_index == 414 {
        return None;
    }
    let mut out = vec![];
    let mut cur = start_node;
    while cur != end_node && out.len() < max_points {
        let (nx, ny) = (nodes[cur].x as i16, nodes[cur].y as i16);
        let id = nodes[cur].id;
        let at = |dx: i16, dy: i16| nodes[idx(nx + dx, ny + dy)].id;
        // the original tests "> 39" for the +x and +y neighbours, which never holds
        if nx > 0 && at(-1, 0) + 5 == id {
            cur = idx(nx - 1, ny);
        } else if nx > 39 && at(1, 0) + 5 == id {
            cur = idx(nx + 1, ny);
        } else if ny > 0 && at(0, -1) + 5 == id {
            cur = idx(nx, ny - 1);
        } else if ny > 39 && at(0, 1) + 5 == id {
            cur = idx(nx, ny + 1);
        } else if nx > 0 && ny > 0 && at(-1, -1) + 7 == id {
            cur = idx(nx - 1, ny - 1);
        } else if nx > 0 && ny < 39 && at(-1, 1) + 7 == id {
            cur = idx(nx - 1, ny + 1);
        } else if nx < 39 && ny > 0 && at(1, -1) + 7 == id {
            cur = idx(nx + 1, ny - 1);
        } else if nx < 39 && ny < 39 && at(1, 1) + 7 == id {
            cur = idx(nx + 1, ny + 1);
        }
        out.push(Vec3::new(start.x + nodes[cur].x as f32 * 0.7, start.y + nodes[cur].y as f32 * 0.7, start.z));
    }
    Some(out)
}

fn add_blockade(b: &Blockade, nodes: &mut [PedPathNode], pos: Vec3) {
    let max_y = b.bb_max.y + 0.3;
    let min_y = b.bb_min.y - 0.3;
    let max_x = b.bb_max.x + 0.3;
    let dx = pos.x - b.pos.x;
    let dy = pos.y - b.pos.y;
    let (c, r) = (b.bound_centre, b.bound_radius);
    if c.x + r >= pos.x && c.y + r >= pos.y && c.x - r <= pos.x + 28.0 && c.y - r <= pos.y + 28.0 {
        for x in 0..GRID {
            let px = x as f32 * 0.7 + dx;
            for y in 0..GRID {
                if !nodes[x * GRID + y].blockade {
                    let py = y as f32 * 0.7 + dy;
                    if max_x > (px * b.right.x + py * b.right.y).abs() {
                        let dot = px * b.forward.x + py * b.forward.y;
                        if max_y > dot && min_y < dot {
                            nodes[x * GRID + y].blockade = true;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathNode {
    pub pos: Vec3,
    pub distance: i16,
    pub object_index: i16,
    pub first_link: i16,
    pub num_links: u8,
    pub unk_bits: u8,
    pub dead_end: bool,
    pub disabled: bool,
    pub between_levels: bool,
    pub group: i8,
    /// Flood-fill and search list links (indices into the node array).
    next: i32,
    prev: i32,
}

impl Default for PathNode {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            distance: MAX_DIST,
            object_index: 0,
            first_link: 0,
            num_links: 0,
            unk_bits: 0,
            dead_end: false,
            disabled: false,
            between_levels: false,
            group: 0,
            next: -1,
            prev: -1,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ConnectionFlags {
    pub crosses_road: bool,
    pub traffic_light: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CarPathLink {
    pub pos: Vec2,
    pub dir: Vec2,
    pub path_node_index: i16,
    pub num_left_lanes: i8,
    pub num_right_lanes: i8,
    pub traffic_light_type: u8,
    pub bridge_lights: bool,
}

impl CarPathLink {
    pub fn one_way_lane_offset(&self) -> f32 {
        if self.num_left_lanes == 0 {
            return 0.5 - 0.5 * self.num_right_lanes as f32;
        }
        if self.num_right_lanes == 0 {
            return 0.5 - 0.5 * self.num_left_lanes as f32;
        }
        0.5
    }
}

/// What the file loader hands over per node (`CPathInfoForObject`).
#[derive(Debug, Clone, Copy, Default)]
struct PathInfo {
    x: i16,
    y: i16,
    z: i16,
    ty: i8,
    next: i8,
    num_left: i8,
    num_right: i8,
    crossing: bool,
}

#[derive(Clone, Copy, Default)]
struct TempNode {
    pos: Vec3,
    dir_x: f32,
    dir_y: f32,
    link1: i16,
    link2: i16,
    num_left: i8,
    num_right: i8,
    link_state: i8,
}

/// A road piece with path nodes (`CTreadable`).
#[derive(Debug, Clone, PartialEq)]
pub struct MapObject {
    pub model: i16,
    pub matrix: Matrix,
    pub node_indices: [[i16; 12]; 2],
}

/// World queries the spawn and wander searches make.
pub trait PathEnv {
    fn is_line_of_sight_clear(&mut self, from: Vec3, to: Vec3) -> bool;
    fn is_sphere_visible(&mut self, pos: Vec3, radius: f32, cam: Option<&Matrix>) -> bool;
    fn find_ground_z(&mut self, p: Vec3) -> (f32, bool);
    /// `TheCamera.GenerationDistMultiplier`.
    fn generation_dist_multiplier(&self) -> f32 {
        1.0
    }
}

#[derive(Debug, Clone)]
pub struct PathFind {
    pub nodes: Vec<PathNode>,
    pub car_path_links: Vec<CarPathLink>,
    pub map_objects: Vec<MapObject>,
    pub object_flags: Vec<u8>,
    pub connections: Vec<i16>,
    pub distances: Vec<i16>,
    pub connection_flags: Vec<ConnectionFlags>,
    pub car_path_connections: Vec<i16>,
    pub num_path_nodes: usize,
    pub num_car_path_nodes: usize,
    pub num_ped_path_nodes: usize,
    pub num_connections: usize,
    pub num_car_path_links: usize,
    pub num_groups: [u8; 2],
    info: [Vec<PathInfo>; 2],
    /// Heads of the 512 search buckets (`next`/`prev` of the head live here).
    search_head: Vec<i32>,
}

impl Default for PathFind {
    fn default() -> Self {
        Self::new(PATHNODESIZE)
    }
}

impl PathFind {
    /// `CPathFind::Init` plus `AllocatePathFindInfoMem(num_path_groups)`.
    pub fn new(num_path_groups: usize) -> Self {
        Self {
            nodes: vec![PathNode::default(); NUM_PATHNODES],
            car_path_links: vec![CarPathLink::default(); NUM_CARPATHLINKS],
            map_objects: vec![],
            object_flags: vec![0; NUM_MAPOBJECTS],
            connections: vec![0; NUM_PATHCONNECTIONS],
            distances: vec![0; NUM_PATHCONNECTIONS],
            connection_flags: vec![ConnectionFlags::default(); NUM_PATHCONNECTIONS],
            car_path_connections: vec![0; NUM_PATHCONNECTIONS],
            num_path_nodes: 0,
            num_car_path_nodes: 0,
            num_ped_path_nodes: 0,
            num_connections: 0,
            num_car_path_links: 0,
            num_groups: [0; 2],
            info: [vec![PathInfo::default(); 12 * num_path_groups], vec![PathInfo::default(); 12 * num_path_groups]],
            search_head: vec![-1; 512],
        }
    }

    /// `RegisterMapObject`.
    pub fn register_map_object(&mut self, model: i16, matrix: Matrix) -> usize {
        self.map_objects.push(MapObject { model, matrix, node_indices: [[-1; 12]; 2] });
        self.map_objects.len() - 1
    }

    /// `StoreNodeInfoPed`. Coordinates are the file's, truncated to 16 bit as the original does.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn store_node_info_ped(&mut self, id: usize, node: usize, ty: i8, next: i8, x: f32, y: f32, z: f32, crossing: bool) {
        self.info[PATH_PED][id * 12 + node] = PathInfo { ty, next, x: x as i16, y: y as i16, z: z as i16, num_left: 0, num_right: 0, crossing };
    }

    /// `StoreNodeInfoCar`.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn store_node_info_car(&mut self, id: usize, node: usize, ty: i8, next: i8, x: f32, y: f32, z: f32, num_left: i8, num_right: i8) {
        self.info[PATH_CAR][id * 12 + node] = PathInfo { ty, next, x: x as i16, y: y as i16, z: z as i16, num_left, num_right, crossing: false };
    }

    /// `IsPathObject`.
    pub fn is_path_object(&self, id: usize) -> bool {
        id < PATHNODESIZE && (self.info[PATH_CAR][id * 12].ty != 0 || self.info[PATH_PED][id * 12].ty != 0)
    }

    fn calc_node_coors(&self, i: PathInfo, obj: usize) -> Vec3 {
        let p = Vec3::new(i.x as f32 / 16.0, i.y as f32 / 16.0, i.z as f32 / 16.0);
        self.map_objects[obj].matrix.transform_point(p)
    }

    pub fn connected_node(&self, id: usize) -> usize {
        self.connections[id] as u16 as usize
    }

    pub fn connection_crosses_road(&self, id: usize) -> bool {
        self.connection_flags[id].crosses_road
    }

    pub fn connection_has_traffic_light(&self, id: usize) -> bool {
        self.connection_flags[id].traffic_light
    }

    pub fn connection_set_traffic_light(&mut self, id: usize) {
        self.connection_flags[id].traffic_light = true;
    }

    /// `PreparePathData`: builds the graph from the stored node info and the registered road pieces.
    pub fn prepare_path_data(&mut self) {
        let mut temp = vec![TempNode::default(); NUMTEMPNODES];
        self.num_connections = 0;
        for n in &mut self.nodes[..PATHNODESIZE] {
            n.unk_bits = 0;
        }
        self.num_path_nodes = 0;
        self.prepare_for_type(PATH_CAR, &mut temp);
        self.num_car_path_nodes = self.num_path_nodes;
        self.prepare_for_type(PATH_PED, &mut temp);
        self.num_ped_path_nodes = self.num_path_nodes - self.num_car_path_nodes;

        for i in 0..self.map_objects.len() {
            let (mut num_extern, mut num_intern, mut num_lanes) = (0, 0, 0);
            let (mut max_x, mut max_y) = (0.0_f32, 0.0_f32);
            for j in 0..12 {
                let k = self.map_objects[i].model as usize * 12 + j;
                let inf = self.info[PATH_CAR][k];
                if inf.ty == NODE_EXTERN {
                    num_extern += 1;
                    num_lanes = num_lanes.max(inf.num_left as i32 + inf.num_right as i32);
                    max_x = max_x.max((inf.x as f32).abs());
                    max_y = max_y.max((inf.y as f32).abs());
                } else if inf.ty == NODE_INTERN {
                    num_intern += 1;
                }
            }
            if num_intern == 1 && num_extern == 2 {
                if num_lanes < 4 {
                    if i & 7 == 4 {
                        self.set_roadblock_flags(i, max_x > max_y);
                    }
                } else {
                    self.set_roadblock_flags(i, max_x > max_y);
                }
            }
        }
        self.count_flood_fill_groups(PATH_CAR);
        self.count_flood_fill_groups(PATH_PED);
        self.info = [vec![], vec![]];
    }

    fn set_roadblock_flags(&mut self, i: usize, east_west: bool) {
        self.object_flags[i] |= USE_IN_ROADBLOCK;
        if east_west {
            self.object_flags[i] |= OBJECT_EAST_WEST;
        } else {
            self.object_flags[i] &= !OBJECT_EAST_WEST;
        }
    }

    /// `CountFloodFillGroups`.
    fn count_flood_fill_groups(&mut self, ty: usize) {
        let (start, end) = if ty == PATH_CAR { (0, self.num_car_path_nodes) } else { (self.num_car_path_nodes, self.num_car_path_nodes + self.num_ped_path_nodes) };
        for n in &mut self.nodes[start..end] {
            n.group = 0;
        }
        let mut n: u16 = 0;
        loop {
            n += 1;
            let Some(i) = (start..end).find(|&i| self.nodes[i].group == 0) else { break };
            self.nodes[i].next = -1;
            self.nodes[i].group = n as i8;
            let mut node = i as i32;
            while node >= 0 {
                let prev = node as usize;
                node = self.nodes[prev].next;
                for k in 0..self.nodes[prev].num_links as usize {
                    let l = self.connected_node(self.nodes[prev].first_link as usize + k);
                    if self.nodes[l].group == 0 {
                        self.nodes[l].group = n as i8;
                        if self.nodes[l].group == 0 {
                            self.nodes[l].group = i8::MIN;
                        }
                        self.nodes[l].next = node;
                        node = l as i32;
                    }
                }
            }
        }
        self.num_groups[ty] = (n - 1) as u8;
    }

    fn prepare_for_type(&mut self, ty: usize, temp: &mut [TempNode]) {
        let maxdist = 1.0_f32;
        let old_num_nodes = self.num_path_nodes;
        let old_num_links = self.num_connections;
        for o in &mut self.map_objects {
            o.node_indices[ty] = [-1; 12];
        }
        let info = std::mem::take(&mut self.info[ty]);

        // internal nodes
        for i in 0..self.map_objects.len() {
            let start = 12 * self.map_objects[i].model as usize;
            for j in 0..12 {
                if info[start + j].ty == NODE_INTERN {
                    let pos = self.calc_node_coors(info[start + j], i);
                    let n = &mut self.nodes[self.num_path_nodes];
                    n.pos = pos;
                    n.object_index = i as i16;
                    n.unk_bits = 1;
                    self.map_objects[i].node_indices[ty][j] = self.num_path_nodes as i16;
                    self.num_path_nodes += 1;
                }
            }
        }

        // external nodes
        let mut list_len = 0;
        for i in 0..self.map_objects.len() {
            let start = 12 * self.map_objects[i].model as usize;
            for j in 0..12 {
                if info[start + j].ty != NODE_EXTERN {
                    continue;
                }
                let coors = self.calc_node_coors(info[start + j], i);
                let mut nearest: i32 = -1;
                let mut nearest_dist = maxdist;
                for (k, t) in temp.iter().enumerate().take(list_len) {
                    if t.link_state != 1 {
                        continue;
                    }
                    let dx = t.pos.x - coors.x;
                    if dx.abs() < nearest_dist {
                        let dy = t.pos.y - coors.y;
                        if dy.abs() < nearest_dist {
                            nearest_dist = dx.abs().max(dy.abs());
                            nearest = k as i32;
                        }
                    }
                }
                let mut next = info[start + j].next as i32;
                if next < 0 {
                    // no link from this node, find the link to it
                    next = 0;
                    let mut k = start;
                    while j as i32 != info[k].next as i32 {
                        k += 1;
                        next += 1;
                    }
                }
                if nearest < 0 {
                    let t = &mut temp[list_len];
                    t.pos = coors;
                    t.link1 = self.map_objects[i].node_indices[ty][next as usize];
                    if ty == PATH_CAR {
                        t.num_left = info[start + j].num_left;
                        t.num_right = info[start + j].num_right;
                    }
                    t.link_state = 1;
                    list_len += 1;
                } else {
                    let nid = nearest as usize;
                    temp[nid].link2 = self.map_objects[i].node_indices[ty][next as usize];
                    temp[nid].link_state = 2;
                    let (l1, l2) = (temp[nid].link1 as usize, temp[nid].link2 as usize);
                    let dx = self.nodes[l1].pos.x - self.nodes[l2].pos.x;
                    let dy = self.nodes[l1].pos.y - self.nodes[l2].pos.y;
                    temp[nid].pos = (temp[nid].pos + coors) * 0.5;
                    let mag = (dx * dx + dy * dy).sqrt();
                    temp[nid].dir_x = dx / mag;
                    temp[nid].dir_y = dy / mag;
                    if ty == PATH_CAR && temp[nid].num_left != 0 && temp[nid].num_right != 0 && (info[start + j].num_left == 0 || info[start + j].num_right == 0) {
                        // the original swaps left and right here
                        temp[nid].num_left = info[start + j].num_right;
                        temp[nid].num_right = info[start + j].num_left;
                    }
                }
            }
        }

        // link the internal nodes
        for i in old_num_nodes..self.num_path_nodes {
            self.nodes[i].num_links = 0;
            self.nodes[i].first_link = self.num_connections as i16;
            for t in temp.iter().take(list_len) {
                if t.link_state != 2 {
                    continue;
                }
                let c = self.num_connections;
                if t.link1 as usize == i {
                    self.connections[c] = t.link2;
                } else if t.link2 as usize == i {
                    self.connections[c] = t.link1;
                } else {
                    continue;
                }
                let other = self.connected_node(c);
                self.distances[c] = (self.nodes[i].pos - self.nodes[other].pos).length() as i16;
                self.connection_flags[c] = ConnectionFlags::default();
                if ty == PATH_CAR {
                    let k = self.find_or_add_link(t.dir_x, t.dir_y, t.pos.x, t.pos.y, i, t.num_left, t.num_right);
                    self.car_path_connections[c] = k as i16;
                }
                self.nodes[i].num_links += 1;
                self.num_connections += 1;
            }

            let obj_i = self.nodes[i].object_index;
            let mut iseg = 0;
            for j in old_num_nodes.max(i.saturating_sub(12))..i {
                if self.nodes[j].object_index == obj_i {
                    iseg += 1;
                }
            }
            let istart = 12 * self.map_objects[obj_i as usize].model as usize;
            for j in old_num_nodes.max(i.saturating_sub(12))..self.num_path_nodes.min(i + 12) {
                if self.nodes[j].object_index != obj_i || i == j {
                    continue;
                }
                let jseg = (j as i32 - i as i32 + iseg) as usize;
                let jstart = 12 * self.map_objects[self.nodes[j].object_index as usize].model as usize;
                let iseg_u = iseg as usize;
                if info[istart + iseg_u].next as i32 == jseg as i32 || info[jstart + jseg].next as i32 == iseg {
                    let c = self.num_connections;
                    self.connections[c] = j as i16;
                    self.distances[c] = (self.nodes[i].pos - self.nodes[j].pos).length() as i16;
                    self.connection_flags[c] = ConnectionFlags::default();
                    if ty == PATH_CAR {
                        let posx = (self.nodes[i].pos.x + self.nodes[j].pos.x) * 0.5;
                        let posy = (self.nodes[i].pos.y + self.nodes[j].pos.y) * 0.5;
                        let mut dx = self.nodes[j].pos.x - self.nodes[i].pos.x;
                        let mut dy = self.nodes[j].pos.y - self.nodes[i].pos.y;
                        let mag = (dx * dx + dy * dy).sqrt();
                        dx /= mag;
                        dy /= mag;
                        if i < j {
                            dx = -dx;
                            dy = -dy;
                        }
                        let k = self.find_or_add_link(dx, dy, posx, posy, i, -1, -1);
                        self.car_path_connections[c] = k as i16;
                    } else {
                        let cross = (info[istart + iseg_u].next as i32 == jseg as i32 && info[istart + iseg_u].crossing) || (info[jstart + jseg].next as i32 == iseg && info[jstart + jseg].crossing);
                        self.connection_flags[c].crosses_road = cross;
                    }
                    self.nodes[i].num_links += 1;
                    self.num_connections += 1;
                }
            }
        }

        if ty == PATH_CAR {
            self.fix_lane_counts();
            // dead ends: nodes with fewer than two ways on
            loop {
                let mut cont = false;
                for i in 0..self.num_path_nodes {
                    self.nodes[i].disabled = false;
                    self.nodes[i].between_levels = false;
                    if !self.nodes[i].dead_end {
                        let k = (0..self.nodes[i].num_links as usize).filter(|&j| !self.nodes[self.connected_node(self.nodes[i].first_link as usize + j)].dead_end).count();
                        if k < 2 {
                            self.nodes[i].dead_end = true;
                            cont = true;
                        }
                    }
                }
                if !cont {
                    break;
                }
            }
        } else {
            self.remove_isolated_ped_nodes(old_num_nodes, old_num_links);
        }
        self.info[ty] = info;
    }

    #[allow(clippy::too_many_arguments)] // the link's fields
    fn find_or_add_link(&mut self, dir_x: f32, dir_y: f32, pos_x: f32, pos_y: f32, node: usize, left: i8, right: i8) -> usize {
        let n = self.car_path_links_len();
        for k in 0..n {
            let l = &self.car_path_links[k];
            if l.dir.x == dir_x && l.dir.y == dir_y && l.pos.x == pos_x && l.pos.y == pos_y {
                return k;
            }
        }
        self.car_path_links[n] = CarPathLink { dir: Vec2::new(dir_x, dir_y), pos: Vec2::new(pos_x, pos_y), path_node_index: node as i16, num_left_lanes: left, num_right_lanes: right, traffic_light_type: 0, bridge_lights: false };
        self.num_car_path_links += 1;
        n
    }

    fn car_path_links_len(&self) -> usize {
        self.num_car_path_links
    }

    fn fix_lane_counts(&mut self) {
        let mut done = false;
        let mut k = 0;
        while !done && k < 10 {
            done = true;
            for i in 0..self.num_path_nodes {
                if self.nodes[i].num_links != 2 {
                    continue;
                }
                let l1 = self.car_path_connections[self.nodes[i].first_link as usize] as usize;
                let l2 = self.car_path_connections[self.nodes[i].first_link as usize + 1] as usize;
                let (a, b) = (self.car_path_links[l1], self.car_path_links[l2]);
                if a.num_left_lanes == -1 && b.num_left_lanes != -1 {
                    done = false;
                    let l = &mut self.car_path_links[l1];
                    if b.path_node_index as usize == i {
                        (l.num_left_lanes, l.num_right_lanes) = (b.num_right_lanes, b.num_left_lanes);
                    } else {
                        (l.num_left_lanes, l.num_right_lanes) = (b.num_left_lanes, b.num_right_lanes);
                    }
                    l.path_node_index = i as i16;
                } else if a.num_left_lanes != -1 && b.num_left_lanes == -1 {
                    done = false;
                    let l = &mut self.car_path_links[l2];
                    if a.path_node_index as usize == i {
                        (l.num_left_lanes, l.num_right_lanes) = (a.num_right_lanes, a.num_left_lanes);
                    } else {
                        (l.num_left_lanes, l.num_right_lanes) = (a.num_left_lanes, a.num_right_lanes);
                    }
                    l.path_node_index = i as i16;
                } else if a.num_left_lanes == -1 && b.num_left_lanes == -1 {
                    done = false;
                }
            }
            k += 1;
        }
        for i in 0..self.num_path_nodes {
            for j in 0..self.nodes[i].num_links as usize {
                let k = self.car_path_connections[self.nodes[i].first_link as usize + j] as usize;
                let l = &mut self.car_path_links[k];
                if l.num_left_lanes < 0 {
                    l.num_left_lanes = 1;
                }
                if l.num_right_lanes < 0 {
                    l.num_right_lanes = 1;
                }
            }
        }
    }

    fn remove_isolated_ped_nodes(&mut self, old_num_nodes: usize, old_num_links: usize) {
        let mut i = old_num_nodes as i32;
        while (i as usize) < self.num_path_nodes {
            let iu = i as usize;
            if self.nodes[iu].num_links != 0 {
                i += 1;
                continue;
            }
            for j in iu..self.num_path_nodes - 1 {
                self.nodes[j] = self.nodes[j + 1];
            }
            for j in old_num_links..self.num_connections {
                let node = self.connected_node(j);
                if node >= iu {
                    self.connections[j] = (node - 1) as i16;
                }
            }
            for o in &mut self.map_objects {
                let a = &mut o.node_indices[PATH_PED];
                for k in 0..12 {
                    if a[k] as i32 == i {
                        for l in k..11 {
                            a[l] = a[l + 1];
                        }
                        a[11] = -1;
                    } else if a[k] as i32 > i {
                        a[k] -= 1;
                    }
                }
            }
            self.num_path_nodes -= 1;
        }
    }

    // ---- queries ----

    /// `CalcRoadDensity`.
    pub fn calc_road_density(&self, x: f32, y: f32) -> f32 {
        let mut density = 0.0;
        for i in 0..self.num_car_path_nodes {
            let n = &self.nodes[i];
            if (n.pos.x - x).abs() < 80.0 && (n.pos.y - y).abs() < 80.0 && n.num_links > 0 {
                for j in 0..n.num_links as usize {
                    let next = self.connected_node(n.first_link as usize + j);
                    let d = self.nodes[next].pos - n.pos;
                    let dist = (d.x * d.x + d.y * d.y).sqrt();
                    let l = &self.car_path_links[self.car_path_connections[n.first_link as usize + j] as usize];
                    density += l.num_left_lanes as f32 * dist;
                    density += l.num_right_lanes as f32 * dist;
                }
            }
        }
        density / 2500.0
    }

    /// `TestForPedTrafficLight`.
    pub fn test_for_ped_traffic_light(&self, n1: usize, n2: usize) -> bool {
        let a = &self.nodes[n1];
        (0..a.num_links as usize).find(|&i| self.connected_node(a.first_link as usize + i) == n2).is_some_and(|i| self.connection_has_traffic_light(a.first_link as usize + i))
    }

    /// `TestCrossesRoad`.
    pub fn test_crosses_road(&self, n1: usize, n2: usize) -> bool {
        let a = &self.nodes[n1];
        (0..a.num_links as usize).find(|&i| self.connected_node(a.first_link as usize + i) == n2).is_some_and(|i| self.connection_crosses_road(a.first_link as usize + i))
    }

    /// `RemoveBadStartNode`: drops the first node if it lies behind the start.
    pub fn remove_bad_start_node(&self, pos: Vec3, nodes: &mut Vec<usize>) {
        if nodes.len() < 2 {
            return;
        }
        let a = self.nodes[nodes[1]].pos - pos;
        let b = self.nodes[nodes[0]].pos - pos;
        if a.x * b.x + a.y * b.y < 0.0 {
            nodes.remove(0);
        }
    }

    /// `SetLinksBridgeLights`.
    pub fn set_links_bridge_lights(&mut self, x1: f32, x2: f32, y1: f32, y2: f32, enable: bool) {
        for l in &mut self.car_path_links[..self.num_car_path_links] {
            if x1 < l.pos.x && l.pos.x < x2 && y1 < l.pos.y && l.pos.y < y2 {
                l.bridge_lights = enable;
            }
        }
    }

    /// `SwitchOffNodeAndNeighbours`.
    pub fn switch_off_node_and_neighbours(&mut self, node: usize, disable: bool) {
        self.nodes[node].disabled = disable;
        if self.nodes[node].num_links < 3 {
            for i in 0..self.nodes[node].num_links as usize {
                let next = self.connected_node(self.nodes[node].first_link as usize + i);
                if self.nodes[next].disabled != disable && self.nodes[next].num_links < 3 {
                    self.switch_off_node_and_neighbours(next, disable);
                }
            }
        }
    }

    /// `SwitchRoadsOffInArea`.
    pub fn switch_roads_off_in_area(&mut self, min: Vec3, max: Vec3, disable: bool) {
        for i in 0..self.num_car_path_nodes {
            let p = self.nodes[i].pos;
            if min.x <= p.x && p.x <= max.x && min.y <= p.y && p.y <= max.y && min.z <= p.z && p.z <= max.z && disable != self.nodes[i].disabled {
                self.switch_off_node_and_neighbours(i, disable);
            }
        }
    }

    /// `SwitchPedRoadsOffInArea`.
    pub fn switch_ped_roads_off_in_area(&mut self, min: Vec3, max: Vec3, disable: bool) {
        for i in self.num_car_path_nodes..self.num_path_nodes {
            let p = self.nodes[i].pos;
            if min.x <= p.x && p.x <= max.x && min.y <= p.y && p.y <= max.y && min.z <= p.z && p.z <= max.z && disable != self.nodes[i].disabled {
                self.switch_off_node_and_neighbours(i, disable);
            }
        }
    }

    /// `SwitchRoadsInAngledArea`. `ty` 0 is the ped graph (the original's test is "not car").
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn switch_roads_in_angled_area(&mut self, p1: Vec3, p2: Vec3, length: f32, ty: u8, mode: u8) {
        let (first, last) = if ty != 0 { (0, self.num_car_path_nodes) } else { (self.num_car_path_nodes, self.num_path_nodes) };
        let (mut z1, mut z2) = (p1.z, p2.z);
        if z1 > z2 {
            std::mem::swap(&mut z1, &mut z2);
        }
        let mut angle = crate::general::radian_angle_between_points(p1.x, p1.y, p2.x, p2.y) + std::f32::consts::FRAC_PI_2;
        while angle < 0.0 {
            angle += std::f32::consts::TAU;
        }
        while angle > std::f32::consts::TAU {
            angle -= std::f32::consts::TAU;
        }
        let mut v12 = Vec2::new(p2.x - p1.x, p2.y - p1.y);
        let len12 = v12.length();
        v12 /= len12;
        let mut v23 = Vec2::new(angle.sin() * length, -(angle.cos() * length));
        v23 /= v23.length();
        let disable = mode == SWITCH_OFF;
        for i in first..last {
            let pos = self.nodes[i].pos;
            if pos.z < z1 || pos.z > z2 {
                continue;
            }
            let d = Vec2::new(pos.x - p1.x, pos.y - p1.y);
            let dot = d.dot(v12);
            if dot < 0.0 || dot > len12 {
                continue;
            }
            let dot = d.dot(v23);
            if dot < 0.0 || dot > length {
                continue;
            }
            if self.nodes[i].disabled != disable {
                self.switch_off_node_and_neighbours(i, disable);
            }
        }
    }

    /// `MarkRoadsBetweenLevelsNodeAndNeighbours`.
    pub fn mark_roads_between_levels_node_and_neighbours(&mut self, node: usize) {
        self.nodes[node].between_levels = true;
        if self.nodes[node].num_links < 3 {
            for i in 0..self.nodes[node].num_links as usize {
                let next = self.connected_node(self.nodes[node].first_link as usize + i);
                if !self.nodes[next].between_levels && self.nodes[next].num_links < 3 {
                    self.mark_roads_between_levels_node_and_neighbours(next);
                }
            }
        }
    }

    /// `MarkRoadsBetweenLevelsInArea`.
    pub fn mark_roads_between_levels_in_area(&mut self, min: Vec3, max: Vec3) {
        for i in 0..self.num_path_nodes {
            let p = self.nodes[i].pos;
            if min.x < p.x && p.x < max.x && min.y < p.y && p.y < max.y && min.z < p.z && p.z < max.z {
                self.mark_roads_between_levels_node_and_neighbours(i);
            }
        }
    }

    /// `PedMarkRoadsBetweenLevelsInArea`.
    pub fn ped_mark_roads_between_levels_in_area(&mut self, min: Vec3, max: Vec3) {
        for i in self.num_car_path_nodes..self.num_path_nodes {
            let p = self.nodes[i].pos;
            if min.x < p.x && p.x < max.x && min.y < p.y && p.y < max.y && min.z < p.z && p.z < max.z {
                self.mark_roads_between_levels_node_and_neighbours(i);
            }
        }
    }

    fn type_range(&self, ty: usize) -> (usize, usize) {
        if ty == PATH_CAR { (0, self.num_car_path_nodes) } else { (self.num_car_path_nodes, self.num_path_nodes) }
    }

    /// `FindNodeClosestToCoors`: -1 if none is closer than `dist_limit`.
    pub fn find_node_closest_to_coors(&self, coors: Vec3, ty: usize, dist_limit: f32, ignore_disabled: bool, ignore_between_levels: bool) -> i32 {
        let (first, last) = self.type_range(ty);
        let mut closest_dist = 10000.0_f32;
        let mut closest = 0;
        for i in first..last {
            let n = &self.nodes[i];
            if ignore_disabled && n.disabled || ignore_between_levels && n.between_levels {
                continue;
            }
            if n.unk_bits == 1 || n.unk_bits == 2 {
                let dist = (n.pos.x - coors.x).abs() + (n.pos.y - coors.y).abs() + 3.0 * (n.pos.z - coors.z).abs();
                if dist < closest_dist {
                    closest_dist = dist;
                    closest = i as i32;
                }
            }
        }
        if closest_dist < dist_limit { closest } else { -1 }
    }

    /// `FindNodeClosestToCoorsFavourDirection`.
    pub fn find_node_closest_to_coors_favour_direction(&self, coors: Vec3, ty: usize, dir_x: f32, dir_y: f32) -> i32 {
        let d = normalise_2d(Vec2::new(dir_x, dir_y));
        let (first, last) = self.type_range(ty);
        let mut closest_dist = 10000.0_f32;
        let mut closest = 0;
        for i in first..last {
            let n = &self.nodes[i];
            if n.unk_bits == 1 || n.unk_bits == 2 {
                let (dx, dy) = (n.pos.x - coors.x, n.pos.y - coors.y);
                let mut dist = dx.abs() + dy.abs() + 3.0 * (n.pos.z - coors.z).abs();
                if dist < closest_dist {
                    let nd = normalise_2d(Vec2::new(dx, dy));
                    dist -= (nd.x * d.x + nd.y * d.y - 1.0) * 20.0;
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest = i as i32;
                    }
                }
            }
        }
        closest
    }

    /// `FindNodeOrientationForCarPlacement`, in degrees.
    pub fn find_node_orientation_for_car_placement(&self, node: usize) -> f32 {
        let n = &self.nodes[node];
        if n.num_links == 0 {
            return 0.0;
        }
        let mut dir = self.nodes[self.connected_node(n.first_link as usize)].pos - n.pos;
        dir.z = 0.0;
        dir = crate::math::normalise(dir);
        heading(dir).to_degrees()
    }

    /// `FindNodeOrientationForCarPlacementFacingDestination`, in degrees.
    pub fn find_node_orientation_facing_destination(&self, node: usize, x: f32, y: f32, towards: bool) -> f32 {
        let n = &self.nodes[node];
        let mut target = Vec3::new(x - n.pos.x, y - n.pos.y, 0.0);
        target = crate::math::normalise(target);
        if n.num_links == 0 {
            return 0.0;
        }
        let mut best_node = self.connected_node(n.first_link as usize);
        let mut best_dot = if towards { -2.0 } else { 2.0 };
        for i in 0..n.num_links as usize {
            let c = self.connected_node(n.first_link as usize + i);
            let mut dir = self.nodes[c].pos - n.pos;
            dir.z = 0.0;
            dir = crate::math::normalise(dir);
            let angle = dir.x * target.x + dir.y * target.y;
            if towards {
                if angle > best_dot {
                    best_dot = angle;
                    best_node = c;
                }
            } else if angle < best_dot {
                best_dot = angle;
                best_node = c;
            }
        }
        let mut dir = self.nodes[best_node].pos - n.pos;
        dir.z = 0.0;
        dir = crate::math::normalise(dir);
        heading(dir).to_degrees()
    }

    /// `NewGenerateCarCreationCoors`: position, the two nodes it lies between, and the fraction towards the second.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn new_generate_car_creation_coors(&self, rng: &mut Rng, x: f32, y: f32, dir_x: f32, dir_y: f32, spawn_dist: f32, angle_limit: f32, forward: bool, ignore_disabled: bool) -> Option<(Vec3, usize, usize, f32)> {
        if self.num_car_path_nodes == 0 {
            return None;
        }
        let dist2d = |p: Vec3| ((p.x - x) * (p.x - x) + (p.y - y) * (p.y - y)).sqrt();
        for _ in 0..500 {
            let node1 = (rng.number() >> 3) as usize % self.num_car_path_nodes;
            let n1 = &self.nodes[node1];
            if n1.disabled && !ignore_disabled {
                continue;
            }
            let dist1 = dist2d(n1.pos);
            if dist1 < spawn_dist + 60.0 {
                let d1 = dist1 - spawn_dist;
                for j in 0..n1.num_links as usize {
                    let node2 = self.connected_node(n1.first_link as usize + j);
                    let n2 = &self.nodes[node2];
                    if n2.disabled && !ignore_disabled {
                        continue;
                    }
                    let d2 = dist2d(n2.pos) - spawn_dist;
                    if d1 * d2 < 0.0 {
                        let f2 = d1.abs() / (d1.abs() + d2.abs());
                        let f1 = 1.0 - f2;
                        let pos = n1.pos * f1 + n2.pos * f2;
                        let d = normalise_2d(Vec2::new(pos.x - x, pos.y - y));
                        let dot = d.x * dir_x + d.y * dir_y;
                        if (forward && dot > angle_limit) || (!forward && dot <= angle_limit) {
                            return Some((pos, node1, node2, f2));
                        }
                    }
                }
            }
        }
        None
    }

    /// `GeneratePedCreationCoors`.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn generate_ped_creation_coors(&self, rng: &mut Rng, env: &mut dyn PathEnv, x: f32, y: f32, mut min_dist: f32, mut max_dist: f32, min_dist_off_screen: f32, max_dist_off_screen: f32, cam: Option<&Matrix>) -> Option<(Vec3, usize, usize, f32)> {
        if self.num_ped_path_nodes == 0 {
            return None;
        }
        for _ in 0..400 {
            let node1 = self.num_car_path_nodes + rng.number() as usize % self.num_ped_path_nodes;
            let n1 = &self.nodes[node1];
            let dx = n1.pos.x - x;
            let dy = n1.pos.y - y;
            if dx * dx + dy * dy < (max_dist + 30.0) * (max_dist + 30.0) {
                if n1.num_links == 0 {
                    continue;
                }
                let link = n1.first_link as usize + rng.number() as usize % n1.num_links as usize;
                if self.connection_crosses_road(link) {
                    continue;
                }
                let node2 = self.connected_node(link);
                if n1.disabled || self.nodes[node2].disabled {
                    continue;
                }
                let f2 = (rng.number() & 0xFF) as f32 / 256.0;
                let f1 = 1.0 - f2;
                let mut pos = n1.pos * f1 + self.nodes[node2].pos * f2;
                let d2 = |p: Vec3| ((p.x - x) * (p.x - x) + (p.y - y) * (p.y - y)).sqrt();
                if d2(pos) < max_dist + 20.0 {
                    pos.x += ((rng.number() & 0xFF) as f32 - 128.0) * 0.01;
                    pos.y += ((rng.number() & 0xFF) as f32 - 128.0) * 0.01;
                    let dist = d2(pos);
                    if !env.is_sphere_visible(pos, 2.0, cam) {
                        min_dist = min_dist_off_screen;
                        max_dist = max_dist_off_screen;
                    }
                    if min_dist < dist && dist < max_dist {
                        let (ground, found) = env.find_ground_z(Vec3::new(pos.x, pos.y, pos.z + 2.0));
                        if !found || (ground - pos.z).abs() > 3.0 {
                            return None;
                        }
                        pos.z = ground;
                        return Some((pos, node1, node2, f2));
                    }
                }
            }
        }
        None
    }

    /// `FindRoadObjectClosestToCoors`: index into `map_objects`.
    pub fn find_road_object_closest_to_coors(&self, coors: Vec3, ty: usize) -> Option<usize> {
        let mut closest = None;
        let mut closest_dist = 10000.0_f32;
        for o in self.map_objects.iter() {
            if o.node_indices[ty][0] < 0 {
                continue;
            }
            let d = o.matrix.pos - coors;
            let f = d.x.abs() + d.y.abs() + d.z.abs();
            if f < 200.0 || f < closest_dist {
                for j in 0..12 {
                    let node1 = o.node_indices[ty][j];
                    if node1 < 0 {
                        break;
                    }
                    let n1 = &self.nodes[node1 as usize];
                    for k in 0..n1.num_links as usize {
                        let node2 = self.connected_node(n1.first_link as usize + k);
                        let n2 = &self.nodes[node2];
                        let line_dist = dist_to_line(n1.pos, n2.pos, coors);
                        if line_dist < closest_dist {
                            closest_dist = line_dist;
                            closest = Some(if (coors - n1.pos).length_squared() < (coors - n2.pos).length_squared() { n1.object_index } else { n2.object_index } as usize);
                        }
                    }
                }
            }
        }
        closest
    }

    /// `FindNextNodeWandering`: the next node and direction (0 north, 2 east, 4 south, 6 west, odd ones between).
    pub fn find_next_node_wandering(&self, env: &mut dyn PathEnv, ty: usize, coors: Vec3, last_node: Option<usize>, cur_dir: u8) -> Option<(usize, u8)> {
        let node = match last_node.filter(|&n| (coors - self.nodes[n].pos).length_squared() <= 7.0) {
            Some(n) => n,
            None => {
                let obj = &self.map_objects[self.find_road_object_closest_to_coors(coors, ty)?];
                let mut best = None;
                let mut node_dist = 1_000_000_000.0_f32;
                for i in 0..12 {
                    if obj.node_indices[ty][i] < 0 {
                        break;
                    }
                    let dist = (coors - self.nodes[obj.node_indices[ty][i] as usize].pos).length_squared();
                    if dist < node_dist {
                        node_dist = dist;
                        best = Some(obj.node_indices[ty][i] as usize);
                    }
                }
                best?
            }
        };
        let cur = Vec2::new((cur_dir as f32 * std::f32::consts::PI / 4.0).sin(), (cur_dir as f32 * std::f32::consts::PI / 4.0).cos());
        let n = &self.nodes[node];
        let mut next_node = None;
        let mut next_dir = 0;
        let mut best_dot = -999999.0_f32;
        for i in 0..n.num_links as usize {
            let next = self.connected_node(n.first_link as usize + i);
            if !n.disabled && self.nodes[next].disabled {
                continue;
            }
            let ped = Vec3::new(coors.x, coors.y, coors.z + 1.0);
            let nc = Vec3::new(self.nodes[next].pos.x, self.nodes[next].pos.y, self.nodes[next].pos.z + 1.0);
            if !env.is_line_of_sight_clear(ped, nc) {
                continue;
            }
            let nd = normalise_2d(Vec2::new(self.nodes[next].pos.x - n.pos.x, self.nodes[next].pos.y - n.pos.y));
            let dot = nd.x * cur.x + nd.y * cur.y;
            if dot >= best_dot {
                next_node = Some(next);
                best_dot = dot;
                next_dir = if nd.x < 0.0 {
                    if 2.0 * nd.y.abs() < -nd.x {
                        6
                    } else if -2.0 * nd.x < nd.y {
                        0
                    } else if 2.0 * nd.x > nd.y {
                        4
                    } else if nd.y > 0.0 {
                        7
                    } else {
                        5
                    }
                } else if 2.0 * nd.y.abs() < nd.x {
                    2
                } else if 2.0 * nd.x < nd.y {
                    0
                } else if -2.0 * nd.x > nd.y {
                    4
                } else if nd.y > 0.0 {
                    1
                } else {
                    3
                };
            }
        }
        Some(next_node.map_or((node, 0), |nn| (nn, next_dir)))
    }

    fn list_add(&mut self, node: usize, list_id: i32) {
        let i = (list_id & 0x1FF) as usize;
        let head_next = self.search_head[i];
        self.nodes[node].next = head_next;
        self.nodes[node].prev = -(i as i32) - 2; // negative values name a bucket head
        if head_next >= 0 {
            self.nodes[head_next as usize].prev = node as i32;
        }
        self.search_head[i] = node as i32;
        self.nodes[node].distance = list_id as i16;
    }

    fn list_remove(&mut self, node: usize) {
        let (prev, next) = (self.nodes[node].prev, self.nodes[node].next);
        if prev >= 0 {
            self.nodes[prev as usize].next = next;
        } else {
            self.search_head[(-prev - 2) as usize] = next;
        }
        if next >= 0 {
            self.nodes[next as usize].prev = prev;
        }
    }

    /// `DoPathSearch`. Returns the nodes to follow (at most `max_nodes`) and the distance.
    /// `start_node` and `target_node` are -1 to search by coordinates; `start_object` is the vehicle's current road piece.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn do_path_search(&mut self, ty: usize, start: Vec3, start_node: i32, target: Vec3, max_nodes: usize, start_object: Option<usize>, dist_limit: f32, target_node: i32) -> (Vec<usize>, f32) {
        let none = (vec![], 100000.0);
        let target_node = if target_node < 0 { self.find_node_closest_to_coors(target, ty, dist_limit, false, false) } else { target_node };
        if target_node < 0 {
            return none;
        }
        let target_node = target_node as usize;
        let (start_obj, paths_to_try);
        if start_node < 0 {
            let so = match start_object.or_else(|| self.find_road_object_closest_to_coors(start, ty)) {
                Some(o) => o,
                None => return none,
            };
            start_obj = so;
            paths_to_try = self.map_objects[so].node_indices[ty].iter().take_while(|&&n| n >= 0).filter(|&&n| self.nodes[n as usize].group == self.nodes[target_node].group).count();
        } else {
            start_obj = self.nodes[start_node as usize].object_index as usize;
            paths_to_try = 1;
        }
        if paths_to_try == 0 {
            return none;
        }
        if start_node < 0 {
            // only the first node's group is checked
            let first = self.map_objects[start_obj].node_indices[ty][0];
            if first < 0 || self.nodes[first as usize].group != self.nodes[target_node].group {
                return none;
            }
        } else if self.nodes[start_node as usize].group != self.nodes[target_node].group {
            return none;
        }

        self.search_head.iter_mut().for_each(|h| *h = -1);
        self.list_add(target_node, 0);
        let mut to_clear = vec![target_node];

        let mut found = 0;
        if start_node < 0 && self.nodes[target_node].object_index as usize == start_obj {
            found += 1;
        }
        let mut i = 0usize;
        while found < paths_to_try {
            let mut node = self.search_head[i];
            while node >= 0 {
                let nu = node as usize;
                if self.nodes[nu].object_index as usize == start_obj && (start_node < 0 || nu == start_node as usize) {
                    found += 1;
                }
                for j in 0..self.nodes[nu].num_links as usize {
                    let link = self.nodes[nu].first_link as usize + j;
                    let next = self.connected_node(link);
                    let dist = self.nodes[nu].distance as i32 + self.distances[link] as i32;
                    if dist < self.nodes[next].distance as i32 {
                        if self.nodes[next].distance != MAX_DIST {
                            self.list_remove(next);
                        }
                        if self.nodes[next].distance == MAX_DIST {
                            to_clear.push(next);
                        }
                        self.list_add(next, dist);
                    }
                }
                let after = self.nodes[nu].next;
                self.list_remove(nu);
                node = after;
            }
            i = (i + 1) & 0x1FF;
        }

        let mut out = vec![];
        let (mut cur, dist);
        if start_node < 0 {
            let mut min_dist = MAX_DIST as i32;
            cur = usize::MAX;
            for &n in self.map_objects[start_obj].node_indices[ty].iter().take_while(|&&n| n >= 0) {
                let nd = (self.nodes[n as usize].pos - start).length() as i32;
                if self.nodes[n as usize].distance as i32 + nd < min_dist {
                    min_dist = self.nodes[n as usize].distance as i32 + nd;
                    cur = n as usize;
                }
            }
            if max_nodes > 0 && cur != usize::MAX {
                out.push(cur);
            }
            dist = min_dist as f32;
        } else {
            cur = start_node as usize;
            dist = self.nodes[cur].distance as f32;
        }
        'trace: while out.len() < max_nodes && cur != target_node && cur != usize::MAX {
            for i in 0..self.nodes[cur].num_links as usize {
                let link = self.nodes[cur].first_link as usize + i;
                let next = self.connected_node(link);
                if self.nodes[cur].distance - self.distances[link] == self.nodes[next].distance {
                    cur = next;
                    out.push(cur);
                    continue 'trace;
                }
            }
            break;
        }
        for n in to_clear {
            self.nodes[n].distance = MAX_DIST;
        }
        (out, dist)
    }

    /// `TestCoorsCloseness`.
    pub fn test_coors_closeness(&mut self, env: &dyn PathEnv, target: Vec3, ty: usize, start: Vec3) -> bool {
        let m = env.generation_dist_multiplier();
        if ty == PATH_CAR {
            let (_, dist) = self.do_path_search(ty, start, -1, target, 32, None, 999_999.9, -1);
            dist < 160.0 * m
        } else {
            let (_, dist) = self.do_path_search(ty, start, -1, target, 0, None, 50.0, -1);
            dist < 100.0 * m
        }
    }

    /// `CPathFind::Save`: bit sets of the disabled and between-levels flags.
    pub fn save(&self) -> Vec<u8> {
        let n = self.num_path_nodes / 8 + 1;
        let mut buf = vec![0u8; 2 * n];
        for i in 0..self.num_path_nodes {
            if self.nodes[i].disabled {
                buf[i / 8] |= 1 << (i % 8);
            }
            if self.nodes[i].between_levels {
                buf[i / 8 + n] |= 1 << (i % 8);
            }
        }
        buf
    }

    /// `CPathFind::Load`.
    pub fn load(&mut self, buf: &[u8]) {
        let n = self.num_path_nodes / 8 + 1;
        for i in 0..self.num_path_nodes {
            self.nodes[i].disabled = buf[i / 8] & (1 << (i % 8)) != 0;
            self.nodes[i].between_levels = buf[i / 8 + n] & (1 << (i % 8)) != 0;
        }
    }
}
