use rentt::add;

#[test]
fn test_add() {
    assert_eq!(add(2, 3), 5);
    assert_eq!(add(1, 1), 2);
}