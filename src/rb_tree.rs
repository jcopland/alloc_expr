use crate::common::{request_memory, PAGE_SIZE};
use crate::large_allocator::LargeAllocator;
use crate::rb_tree::Colour::{Black, Red};
use crate::rb_tree::Direction::{Left, Right};
use std::alloc::Layout;
use std::ptr::NonNull;

#[derive(PartialEq)]
enum Colour {
    Red,
    Black,
}

#[derive(Clone, Copy)]
enum Direction {
    Left,
    Right,
}

impl Direction {
    fn flip(self) -> Self {
        match self {
            Left => Right,
            Right => Left,
        }
    }
}

impl From<bool> for Direction {
    fn from(value: bool) -> Self {
        if value {
            Right
        } else {
            Left
        }
    }
}

type NodePtr<T> = Option<NonNull<Node<T>>>;

struct Node<T: Ord> {
    key: T,
    colour: Colour,
    links: [NodePtr<T>; 2],
}

impl<T: Ord> Node<T> {
    fn new(key: T) -> NonNull<Node<T>> {
        let layout = Layout::new::<Node<T>>()
            .align_to(PAGE_SIZE)
            .expect("Failed to align layout");
        unsafe {
            let ptr: NonNull<Node<T>> = request_memory(layout.size()).cast();
            let node: Node<T> = Node {
                key,
                colour: Colour::Red,
                links: [None, None],
            };
            ptr.as_ptr().write(node);
            ptr
        }
    }

    fn link(&self, dir: Direction) -> NodePtr<T> {
        self.links[dir as usize]
    }

    fn set_link(&mut self, dir: Direction, node: NodePtr<T>) {
        self.links[dir as usize] = node;
    }

    fn is_red(node: NodePtr<T>) -> bool {
        node.map_or(false, |n| unsafe { n.as_ref().colour == Colour::Red })
    }

    fn single_rotation(&mut self, dir: Direction) -> NonNull<Node<T>> {
        let opposite_dir = dir.flip();
        let mut child = self.link(opposite_dir).unwrap();
        unsafe {
            self.set_link(opposite_dir, child.as_ref().link(dir));
            self.colour = Colour::Red;
            child.as_mut().colour = Colour::Black;
            child.as_mut().set_link(dir, Some(NonNull::from(self)));
            child
        }
    }

    fn double_rotation(&mut self, dir: Direction) -> NonNull<Node<T>> {
        unsafe {
            let mut child = self.link(dir.flip()).unwrap();
            let grand_child = child.as_mut().single_rotation(dir.flip());
            self.set_link(dir.flip(), Some(grand_child));
            self.single_rotation(dir)
        }
    }
}

pub struct RBTree<T: Ord + Default> {
    root: NodePtr<T>,
}

impl<T: Ord + Default> RBTree<T> {
    pub fn new() -> Self {
        Self { root: None }
    }

    pub fn insert(&mut self, key: T) {
        let node = Node::new(key);
        unsafe { self.insert_helper(node) };
    }

    pub fn pop(&mut self, key: &T) -> NodePtr<T> {
        unsafe { self.pop_helper(key) }
    }

    unsafe fn insert_helper(&mut self, node: NonNull<Node<T>>) {
        if self.root.is_none() {
            self.root = Some(node);
            return;
        }

        let mut current = self.root;
        let mut parent: NodePtr<T> = None;
        let mut grandparent: NodePtr<T> = None;
        let mut direction = Left;
        let mut last = direction;

        let key = &node.as_ref().key;

        loop {
            if current.is_none() {
                parent.unwrap().as_mut().set_link(direction, Some(node));
                break;
            }

            let mut curr_node = current.unwrap().as_mut();

            if Node::is_red(curr_node.link(Left)) && Node::is_red(curr_node.link(Right)) {
                curr_node.colour = Colour::Red;
                curr_node.link(Left).unwrap().as_mut().colour = Colour::Black;
                curr_node.link(Right).unwrap().as_mut().colour = Colour::Black;
            }

            if Node::is_red(current) && Node::is_red(parent) {
                let dir2 = Direction::from(grandparent.unwrap().as_ref().link(Right) == parent);
                if current == parent.unwrap().as_ref().link(last) {
                    grandparent
                        .unwrap()
                        .as_mut()
                        .set_link(dir2, Some(curr_node.single_rotation(last.flip())));
                } else {
                    grandparent
                        .unwrap()
                        .as_mut()
                        .set_link(dir2, Some(curr_node.double_rotation(last.flip())));
                }
            }

            // todo: I need to make this a linked list or munmap here or something to handle duplicate inserts
            if curr_node.key == *key {
                break;
            }

            last = direction;
            direction = Direction::from(curr_node.key < *key);

            if grandparent.is_some() {
                grandparent = parent;
            }
            parent = current;
            current = curr_node.link(direction);
        }

        // Final adjustments after insertion
        self.root.unwrap().as_mut().colour = Black;
    }

    /// three stage function that should be pulled into three functions
    /// 1) iteratively walks the tree until finding the lowerbound
    /// 2) iteratively walks the tree until finding inorder successor (go right, then get minimum)
    /// 3) rearranges pointers such that the result node isn't the child of anything
    /// todo: this 90 lines of unsafe code, it can almost certainly be refactored
    unsafe fn pop_helper(&mut self, key: &T) -> NodePtr<T> {
        let mut new_node = Node {
            key: T::default(),
            colour: Red,
            links: [None, self.root],
        };

        let mut current = NonNull::new(&mut new_node);
        // parent of current node
        let mut parent: NodePtr<T> = None;
        // grandparent of current node
        let mut grandparent: NodePtr<T> = None;
        // direction to iterate through tree on next iteration
        let mut direction = Left;
        // previous direction
        let mut last = direction;
        let mut result: NodePtr<T> = None;
        let mut result_parent = None;
        let mut result_direction = Left;

        // eagerly reorder the tree
        while let Some(node) = current.as_ref() {
            last = direction;
            grandparent = parent;
            parent = current;
            direction = (node.as_ref().key < *key).into();

            // get the lower bound, if it exists
            if node.as_ref().key == *key
                || (node.as_ref().key > *key && node.as_ref().links[0].is_none())
            {
                result = Some(*node);
                result_direction = last;
                result_parent = parent;
            }

            current = Some(*node);

            let curr_node = current.unwrap().as_mut();

            if !Node::is_red(current) && Node::is_red(Node::link(curr_node, direction)) {
                let parent_node = parent.unwrap().as_mut();

                if Node::is_red(Node::link(curr_node, direction.flip())) {
                    let rotated = curr_node.single_rotation(direction);
                    parent_node.set_link(last, Some(rotated));
                    parent = Some(rotated);
                } else {
                    let s = parent_node.link(last.flip());

                    if s.is_none() {
                        continue;
                    }
                    let s_node = s.unwrap().as_mut();
                    if !Node::is_red(s_node.links[0]) && Node::is_red(s_node.links[1]) {
                        parent_node.colour = Black;
                        s_node.colour = Red;
                        curr_node.colour = Red;
                    } else {
                        let gp_node = grandparent.unwrap().as_mut();
                        let new_dir = (gp_node.links[1] == parent).into();

                        let rotated = if Node::is_red(s_node.link(last)) {
                            parent_node.double_rotation(last)
                        } else {
                            parent_node.single_rotation(last)
                        };

                        gp_node.set_link(new_dir, Some(rotated));

                        let node = gp_node.link(new_dir).unwrap().as_mut();
                        // rotate colours here
                        curr_node.colour = Red;
                        node.colour = Red;
                        node.links[0].unwrap().as_mut().colour = Black;
                        node.links[1].unwrap().as_mut().colour = Black;
                    }
                }
            }
        }

        // at this point the target has been found, clean up links
        if result.is_some() {
            Self::extract_node(
                result_parent.unwrap().as_mut(),
                result_direction,
                current.unwrap().as_mut(),
                parent.unwrap().as_mut(),
                result.unwrap().as_mut(),
            )
        }

        result
    }

    /// this swaps pointers around the tree for an efficient node removal. The parent of the
    /// target node now points to the in order successor, and the parent of the successor now points
    /// to the child (if it exists) of the in order successor.
    unsafe fn extract_node(
        result_parent: *mut Node<T>,
        result_dir: Direction,
        current: &mut Node<T>,
        parent: *mut Node<T>,
        result: *mut Node<T>,
    ) {
        // null check. todo: prove this is a waste of an if check
        if result_parent.is_null() || parent.is_null() || result.is_null() {
            return;
        }
        // update the result parent pointer to point to the current node, which at this point
        // is the inorder successor
        (*result_parent).set_link(result_dir, NonNull::new(current));
        // if a child exists it exists in at maximum one branch, this finds the direction
        let current_child_direction = current.links[0].is_none().into();
        // update parent of successor node to point to potential current node children
        let current_child = current.link(current_child_direction);

        match (*parent).link(Left) {
            // current child is on the right
            None => {
                (*parent).links[1] = current_child;
            }
            Some(node) => {
                let direction = (node.as_ptr() == current).into();
                (*parent).set_link(direction, current_child);
            }
        };

        // finally, update what the current node points to
        current.links[0] = (*result).link(Left);
        current.links[1] = (*result).link(Right);
    }
}
