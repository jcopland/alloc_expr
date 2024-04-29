use std::alloc::{alloc, Layout};
use std::cmp::{max, Ordering};
use std::mem::{size_of, swap};
use std::ptr::NonNull;
use crate::avl_tree::DeleteAction::{NoAction, SearchDelete};
use crate::avl_tree::SearchDirection::{Left, Right, Root};
use crate::common::{request_memory, PAGE_SIZE};
use crate::large_allocator::LargeAllocator;

/// When deleting in a binary search tree, to prevent keeping a parent pointer this
/// enum enables the delete function the ability to know exactly what action to take upon
/// finding the correct node to remove; this is important as it limits the amount of recrusive
/// searches and also guarantees a correct outcome in the case of searching for a lower bound, as
/// there might be multiple "lowerbounds"
#[derive(Debug)]
enum DeleteAction {
    /// deletion was taken care of
    NoAction,
    /// search to delete again, as a root node has been swapped with its inorder successor
    SearchDelete
}

#[derive(Clone, Copy)]
enum SearchDirection {
    Left(NonNull<Node>),
    Right(NonNull<Node>),
    Root,
}

type NodePtr = Option<NonNull<Node>>;

#[derive(Debug)]
struct AvlHeader {
    size: usize,
    height: i32,
    left: NodePtr,
    right: NodePtr,
}

#[derive(Debug)]
struct Node {
    header: AvlHeader,
    data: *mut u8,
}

pub struct AVLTree {
    root: NodePtr,
}

impl Node {

    unsafe fn new(layout: Layout) -> NonNull<Node> {
        let header_layout = Layout::new::<AvlHeader>();
        let (total_layout, offset) = header_layout.extend(layout).unwrap();

        let page_aligned_layout = total_layout.pad_to_align().align_to(PAGE_SIZE).unwrap();

        // this can fail, but realistically we have 256 tib and I'm not writing a program that's
        // getting near that any time soon with this malloc :)
        let address = request_memory(page_aligned_layout.size());

        let node_ptr: NonNull<Node> = address.cast();

        let header = AvlHeader {
            size: page_aligned_layout.size() - size_of::<AvlHeader>(),
            height: 1,
            left: None,
            right: None,
        };

        // Calculate data pointer
        let data_ptr = address.as_ptr().add(offset);

        // Write node to memory
        node_ptr.as_ptr().write(Node {
            header,
            data: data_ptr
        });

        node_ptr
    }

    fn height(node: NodePtr) -> i32 {
        node.map_or(0, |node| unsafe { node.as_ref().header.height })
    }

    fn update_height(&mut self) {
        self.header.height = max(Self::height(self.header.left), Self::height(self.header.right)) + 1;
    }

    fn balance_factor(&self) -> i32 {
        Self::height(self.header.left) - Self::height(self.header.right)
    }

    unsafe fn rotate_right(ptr: &mut NonNull<Node>) -> NonNull<Node> {
        let mut left_ptr = ptr.as_mut().header.left.take().unwrap();
        let left_right = left_ptr.as_mut().header.right.take();

        ptr.as_mut().header.left = left_right;
        ptr.as_mut().update_height();

        left_ptr.as_mut().header.right = Some(*ptr);
        left_ptr.as_mut().update_height();

        left_ptr
    }

    unsafe fn rotate_left(ptr: &mut NonNull<Node>) -> NonNull<Node> {
        let mut right_ptr = ptr.as_mut().header.right.take().unwrap();
        let right_left = right_ptr.as_mut().header.left.take();

        ptr.as_mut().header.right = right_left;
        ptr.as_mut().update_height();

        right_ptr.as_mut().header.left = Some(*ptr);
        right_ptr.as_mut().update_height();

        right_ptr
    }

    unsafe fn rebalance(ptr: &mut NonNull<Node>) -> NonNull<Node> {
        ptr.as_mut().update_height();
        let balance = ptr.as_ref().balance_factor();

        if balance > 1 {
            if ptr.as_ref().header.left.map_or(false, |left| left.as_ref().balance_factor() < 0) {
                ptr.as_mut().header.left = Some(Self::rotate_left(&mut ptr.as_ref().header.left.unwrap()));
            }
            Self::rotate_right(ptr)
        } else if balance < -1 {
            if ptr.as_ref().header.right.map_or(false, |right| right.as_ref().balance_factor() > 0) {
                ptr.as_mut().header.right = Some(Self::rotate_right(&mut ptr.as_ref().header.right.unwrap()));
            }
            Self::rotate_left(ptr)
        } else {
            *ptr
        }
    }

    unsafe fn get_min(node: NonNull<Node>, parent: NonNull<Node>) -> (NonNull<Node>, NonNull<Node>) {
        let mut current = node;
        let mut parent = parent;
        while let Some(left) = current.as_ref().header.left {
            parent = current;
            current = left;
        }
        (current, parent)
    }


    fn swap_header_details(&mut self, other: &mut Node) {
        swap(&mut self.header.left, &mut other.header.left);
        swap(&mut self.header.right, &mut other.header.right);
        swap(&mut self.header.height, &mut other.header.height);
    }

}

impl AVLTree {
    fn new() -> Self {
        AVLTree { root: None }
    }

    fn insert_node(&mut self, value: NonNull<Node>) {
        let root= self.reinsert_node(self.root, value);
        self.root = Some(root);
    }

    fn remove(&mut self, value: usize) -> NodePtr {
        if let Some(node) = self.root {
            unsafe { self.remove_node(node, value, Root) }
        } else {
            None
        }
    }

    unsafe fn swap_nodes(
        &mut self,
        mut target_node: NonNull<Node>,
        mut successor_node: NonNull<Node>,
        mut successor_parent: NonNull<Node>,
        parent: SearchDirection
    ) -> DeleteAction
    {
        // in all cases swap the meta data, height, left pointer, right pointer, and make parent
        // point at successor
        target_node.as_mut().swap_header_details(&mut successor_node.as_mut());
        match parent {
            Left(mut left) => left.as_mut().header.left = Some(successor_node),
            Right(mut right) => right.as_mut().header.right = Some(successor_node),
            Root => self.root = Some(successor_node),
        }

        // println!("{}", successor_node.as_ref());
        // println!("suc right: {} suc left: {}", successor_node.as_ref().header.right.unwrap().as_ref(), successor_node.as_ref().header.left.unwrap().as_ref());

        // remove self loop
        if successor_parent == target_node {
            // stop the recursive loop formed here
            if successor_node.as_mut().header.right == Some(successor_node) {
                successor_node.as_mut().header.right = None;
            } else {
                successor_node.as_mut().header.left = None;
            }

            NoAction
        } else {
            // make successor parent point to target
            // todo: prove that it's always going to be pointing left and remove a pointless
            // if check
            if successor_parent.as_mut().header.right == Some(successor_node) {
                successor_parent.as_mut().header.right = Some(target_node);
            }
            if successor_parent.as_mut().header.left == Some(successor_node) {
                successor_parent.as_mut().header.left = Some(target_node);
            }
            SearchDelete
        }

    }

    /// Delete the node by removing pointers pointing to it. this function returns a DeleteAction
    /// so the caller knows whether or not it needs to search through the tree and delete again
    /// in the case of a swap between the node being deleted, and the next in order node in the
    /// tree.
    unsafe fn delete_node(&mut self, mut root_node: NonNull<Node>, parent: SearchDirection) -> DeleteAction {
        let ref_node = root_node.as_mut();
        if ref_node.header.left.is_none() && ref_node.header.right.is_none() {
            // with no children return true, and the level above deletes this node when unwinding
            // the recursive stack
            match parent {
                Left(mut node) => node.as_mut().header.left = None,
                Right(mut node) => node.as_mut().header.right = None,
                Root => self.root = None
            }
            NoAction
        } else if ref_node.header.left.is_some() && ref_node.header.right.is_some() {
            // with both children swap this node and the minimum node on the right hand side
            // todo: find a better way of doing this, way too many pointers

            // safe because of if check
            let right_child = ref_node.header.right.unwrap();
            let (successor, successor_parent) = Node::get_min(right_child, root_node);

            self.swap_nodes(root_node, successor, successor_parent, parent)
        } else {
            // A single child node remains, swap it and then remove the node
            let successor = root_node.as_mut().header.left.or(root_node.as_mut().header.right).unwrap();
            self.swap_nodes(root_node, successor, root_node, parent)
        }
    }

    /// starting from root, search until the appropriate node is found, and then remove. A parent
    /// is provided as this allocator depends on chunks of memory being contiguous; a pointer to
    /// data MUST be the size of an AVLHeader in front of the beginning of the AVLHeader. In other
    /// words, it will cause serious issues if a complete swap doesn't occur upon an end user
    /// calling free() upon some data. For this reason if the parent is None, it's the root node.
    ///
    /// This takes the lower bound of a size in order to implement a best fit approach.
    unsafe fn remove_node(&mut self,
                          mut root_node: NonNull<Node>,
                          size: usize,
                          parent: SearchDirection) -> NodePtr
    {
        let ref_node = root_node.as_mut();

        let mut header = &mut ref_node.header;
        let result;

        match size.cmp(&header.size) {
            Ordering::Less => {
                if let Some(node) = header.left {
                    result = self.remove_node(node, size, Left(root_node))
                } else {
                    // at this point no better fit exists
                    result = Some(root_node);
                }
            }
            Ordering::Greater => {
                if let Some(node) = header.right {
                    result = self.remove_node(node, size, Right(root_node))
                } else {
                    result = None;
                }
            }
            Ordering::Equal => {
                result = Some(root_node);
                let right_child = ref_node.header.right;
                let delete_action = self.delete_node(root_node, parent);
                match delete_action {
                    // because of tree guarantees, look right. This is important for correctness
                    // as swapping with the successor will force the search to fail. This call
                    // has to be done here for rebalancing logic
                    SearchDelete => {
                        self.remove_node(right_child.unwrap(), size, parent);

                        // update the root node in the stack after recursive delete
                        match parent {
                            Left(parent) => root_node = parent,
                            Right(parent) => root_node = parent,
                            Root => root_node = self.root.unwrap(),
                        }
                    },
                    _ => {},
                }
            }
        }
        Node::rebalance(&mut root_node);
        result
    }

    fn reinsert_node(&mut self, node: NodePtr, value: NonNull<Node>) -> NonNull<Node> {
        match node {
            None => value,
            Some(mut ptr) => {
                let node_ref =  unsafe {ptr.as_mut() };
                let value_ref = unsafe { value.as_ref() };
                match value_ref.header.size.cmp(&node_ref.header.size) {
                    Ordering::Less => node_ref.header.left = Some(self.reinsert_node(node_ref.header.left, value)),
                    Ordering::Greater => node_ref.header.right = Some(self.reinsert_node(node_ref.header.right, value)),
                    // todo: handle this case, could be a linked list of nodes potentially, not sure, but it seems likely
                    // that in a real malloc implementation there'd be multiple chunks the same size
                    // perhaps a non stupid implementation involves calling munmap here and letting
                    // the os deal with it?
                    Ordering::Equal => return ptr,
                }
                unsafe { Node::rebalance(&mut ptr) }
            }
        }
    }
}

unsafe impl LargeAllocator for AVLTree {
    // todo: I should consider making this more robust, I could have node creation return a result
    // of AllocError from nightly and then on that return a null ptr
    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        match self.remove(layout.size()) {
            None => {
                let node = Node::new(layout);
                node.as_ref().data
            }
            Some(node) => {
                node.as_ref().data
            }
        }
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        assert!(!ptr.is_null(), "Attempted to deallocate a null pointer.");

        // walk backwards, get the data required
        let address = ptr.sub(size_of::<AvlHeader>());

        // this already has been aligned
        let node: NonNull<Node> = *address.cast();

        // put the mmapped memory back in the tree
        self.insert_node(node);
    }
    unsafe fn realloc(&mut self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        assert!(!ptr.is_null(), "Attempted to reallocate a null pointer.");

        // walk backwards, get the data required
        let address = ptr.sub(size_of::<AvlHeader>());

        // this already has been aligned
        let node: NonNull<Node> = *address.cast();

        // todo: Should I get a chunk here if necessary? I'm leaning on virtual memory here
        if node.as_ref().header.size >= new_size {
            return ptr;
        }

        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap();
        let new_ptr = alloc(new_layout);

        if new_ptr.is_null() {
            return std::ptr::null_mut(); // Return null on allocation failure.
        }

        // Copy the existing data to the new location.
        std::ptr::copy_nonoverlapping(ptr, new_ptr, node.as_ref().header.size);

        self.dealloc(ptr);

        new_ptr
    }}
