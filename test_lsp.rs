fn main() {
    let x = 42;
    println!("Value: {}", x);
}

fn helper() -> i32 {
    // Missing return - should show diagnostic
}
