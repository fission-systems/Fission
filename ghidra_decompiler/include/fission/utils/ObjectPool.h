#pragma once

#include <vector>
#include <cstddef>
#include <new>

namespace fission {

template <typename T, size_t BlockSize = 8192>
class ThreadLocalObjectPool {
private:
  // Use alignas(T) so the node fully satisfies T's alignment requirements.
  union alignas(T) Node {
    char data[sizeof(T)];
    Node* next;
  };

  Node* freeList = nullptr;
  std::vector<Node*> blocks;

  void allocateBlock() {
    Node* newBlock = ::new Node[BlockSize];
    blocks.push_back(newBlock);
    // Link the free-list chain.
    for (size_t i = 0; i < BlockSize - 1; ++i) {
      newBlock[i].next = &newBlock[i + 1];
    }
    newBlock[BlockSize - 1].next = freeList;
    freeList = newBlock;
  }

public:
  ThreadLocalObjectPool() = default;

  ~ThreadLocalObjectPool() {
    for (Node* block : blocks) {
      ::delete[] block;
    }
  }

  void* alloc() {
    if (!freeList) {
      allocateBlock();
    }
    Node* node = freeList;
    freeList = freeList->next;
    return node;
  }

  void free(void* ptr) {
    if (!ptr) return;
    Node* node = static_cast<Node*>(ptr);
    node->next = freeList;
    freeList = node;
  }
};

}  // namespace fission
