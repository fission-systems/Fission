#[derive(Debug, Clone)]
struct Record {
    id: i32,
    value: i32,
    name: String,
}

impl Record {
    fn new(id: i32, value: i32, name: &str) -> Self {
        Record {
            id,
            value,
            name: name.to_string(),
        }
    }

    fn process(&self) -> i32 {
        self.id + self.value
    }
}

#[derive(Debug)]
struct Node {
    data: i32,
    next: Option<Box<Node>>,
    prev: Option<Box<Node>>,
}

impl Node {
    fn new(data: i32) -> Self {
        Node {
            data,
            next: None,
            prev: None,
        }
    }
}

struct LinkedList {
    head: Option<Box<Node>>,
}

impl LinkedList {
    fn new() -> Self {
        LinkedList { head: None }
    }

    fn insert_at_head(&mut self, data: i32) {
        let mut new_node = Box::new(Node::new(data));
        new_node.next = self.head.take();
        self.head = Some(new_node);
    }

    fn sum(&self) -> i32 {
        let mut sum = 0;
        let mut current = &self.head;

        while let Some(node) = current {
            sum += node.data;
            current = &node.next;
        }

        sum
    }

    fn length(&self) -> i32 {
        let mut count = 0;
        let mut current = &self.head;

        while let Some(node) = current {
            count += 1;
            current = &node.next;
        }

        count
    }
}

#[derive(Debug, Clone)]
struct Point {
    x: f64,
    y: f64,
    z: f64,
}

impl Point {
    fn new(x: f64, y: f64, z: f64) -> Self {
        Point { x, y, z }
    }

    fn distance_from_origin(&self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    fn translate(&mut self, dx: f64, dy: f64, dz: f64) {
        self.x += dx;
        self.y += dy;
        self.z += dz;
    }
}

struct RecordManager {
    records: Vec<Record>,
}

impl RecordManager {
    fn new() -> Self {
        RecordManager {
            records: Vec::new(),
        }
    }

    fn add_record(&mut self, rec: Record) {
        self.records.push(rec);
    }

    fn get_total_value(&self) -> i32 {
        self.records.iter().map(|r| r.value).sum()
    }

    fn count(&self) -> usize {
        self.records.len()
    }
}

struct VectorOps {
    data: Vec<i32>,
}

impl VectorOps {
    fn new(data: Vec<i32>) -> Self {
        VectorOps { data }
    }

    fn sum(&self) -> i32 {
        self.data.iter().sum()
    }

    fn modify_at(&mut self, index: usize, value: i32) {
        if index < self.data.len() {
            self.data[index] = value;
        }
    }
}

fn main() {
    // Test Record
    let rec1 = Record::new(1, 100, "Alice");
    let rec2 = Record::new(2, 200, "Bob");
    let calc1 = rec1.process();
    let calc2 = rec2.process();

    // Test LinkedList
    let mut list = LinkedList::new();
    list.insert_at_head(10);
    list.insert_at_head(20);
    list.insert_at_head(30);
    list.insert_at_head(40);
    let list_sum = list.sum();
    let list_len = list.length();

    // Test Point
    let mut point1 = Point::new(1.0, 2.0, 3.0);
    let dist = point1.distance_from_origin();
    point1.translate(1.0, 1.0, 1.0);

    // Test RecordManager
    let mut manager = RecordManager::new();
    manager.add_record(rec1.clone());
    manager.add_record(rec2.clone());
    let total_value = manager.get_total_value();
    let rec_count = manager.count();

    // Test VectorOps
    let arr = vec![1, 2, 3, 4, 5];
    let mut vec_ops = VectorOps::new(arr);
    let vec_sum = vec_ops.sum();
    vec_ops.modify_at(0, 10);

    println!("Structs and pointers test: {} {} {} {} {}", 
        calc1, list_sum, total_value, dist as i32, vec_sum);
    println!("List length: {}, Record count: {}", list_len, rec_count);
}
