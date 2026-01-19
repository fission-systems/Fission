class FissionSwiftTester {
    var name: String
    var counter: Int

    init(name: String) {
        self.name = name
        self.counter = 0
    }

    func sayHello() {
        print("Hello from Swift, \(name)!")
    }

    func increment() -> Int {
        counter += 1
        return counter
    }
}

let tester = FissionSwiftTester(name: "FissionUser")
tester.sayHello()
print("Counter: \(tester.increment())")
