// Medium C Binaries - Enhanced Algorithm Library
// Includes: Graph algorithms (Dijkstra, BFS, DFS, Topological Sort), DP, String processing, Encryption

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <stdint.h>

// ============================================================================
// Graph Algorithms
// ============================================================================

#define MAX_NODES 100
#define MAX_EDGES 500
#define INF 1000000

typedef struct {
    int src, dest, weight;
} Edge;

typedef struct {
    int nodes;
    int edges;
    Edge edge_list[MAX_EDGES];
    int adj_matrix[MAX_NODES][MAX_NODES];
} Graph;

// Dijkstra's shortest path algorithm
void dijkstra(Graph *g, int start, int *dist) {
    int visited[MAX_NODES] = {0};
    
    for (int i = 0; i < g->nodes; i++) {
        dist[i] = INF;
    }
    dist[start] = 0;
    
    for (int i = 0; i < g->nodes; i++) {
        int u = -1;
        int min_dist = INF;
        
        for (int v = 0; v < g->nodes; v++) {
            if (!visited[v] && dist[v] < min_dist) {
                min_dist = dist[v];
                u = v;
            }
        }
        
        if (u == -1) break;
        visited[u] = 1;
        
        for (int v = 0; v < g->nodes; v++) {
            if (g->adj_matrix[u][v] != 0) {
                int new_dist = dist[u] + g->adj_matrix[u][v];
                if (new_dist < dist[v]) {
                    dist[v] = new_dist;
                }
            }
        }
    }
}

// DFS traversal
void dfs(Graph *g, int node, int *visited, int *path, int *path_len) {
    visited[node] = 1;
    path[(*path_len)++] = node;
    
    for (int i = 0; i < g->nodes; i++) {
        if (g->adj_matrix[node][i] != 0 && !visited[i]) {
            dfs(g, i, visited, path, path_len);
        }
    }
}

// BFS (Breadth-First Search) traversal
void bfs(Graph *g, int start, int *visited, int *path, int *path_len) {
    int queue[MAX_NODES];
    int front = 0, rear = 0;
    
    visited[start] = 1;
    queue[rear++] = start;
    
    while (front < rear) {
        int node = queue[front++];
        path[(*path_len)++] = node;
        
        for (int i = 0; i < g->nodes; i++) {
            if (g->adj_matrix[node][i] != 0 && !visited[i]) {
                visited[i] = 1;
                queue[rear++] = i;
            }
        }
    }
}

// Topological Sort using DFS
void topological_sort_dfs(Graph *g, int node, int *visited, int *stack, int *top) {
    visited[node] = 1;
    
    for (int i = 0; i < g->nodes; i++) {
        if (g->adj_matrix[node][i] != 0 && !visited[i]) {
            topological_sort_dfs(g, i, visited, stack, top);
        }
    }
    
    stack[(*top)++] = node;
}

void topological_sort(Graph *g, int *result) {
    int visited[MAX_NODES] = {0};
    int stack[MAX_NODES];
    int top = 0;
    
    for (int i = 0; i < g->nodes; i++) {
        if (!visited[i]) {
            topological_sort_dfs(g, i, visited, stack, &top);
        }
    }
    
    int idx = 0;
    for (int i = top - 1; i >= 0; i--) {
        result[idx++] = stack[i];
    }
}

// Floyd-Warshall algorithm
void floyd_warshall(Graph *g, int dist[MAX_NODES][MAX_NODES]) {
    // Initialize
    for (int i = 0; i < g->nodes; i++) {
        for (int j = 0; j < g->nodes; j++) {
            if (i == j) {
                dist[i][j] = 0;
            } else if (g->adj_matrix[i][j] != 0) {
                dist[i][j] = g->adj_matrix[i][j];
            } else {
                dist[i][j] = INF;
            }
        }
    }
    
    // Main algorithm
    for (int k = 0; k < g->nodes; k++) {
        for (int i = 0; i < g->nodes; i++) {
            for (int j = 0; j < g->nodes; j++) {
                if (dist[i][k] + dist[k][j] < dist[i][j]) {
                    dist[i][j] = dist[i][k] + dist[k][j];
                }
            }
        }
    }
}

// ============================================================================
// Dynamic Programming Algorithms
// ============================================================================

// Longest Common Subsequence
int lcs_length(const char *s1, const char *s2) {
    int m = strlen(s1);
    int n = strlen(s2);
    int **dp = (int **)malloc((m + 1) * sizeof(int *));
    
    for (int i = 0; i <= m; i++) {
        dp[i] = (int *)malloc((n + 1) * sizeof(int));
        for (int j = 0; j <= n; j++) {
            dp[i][j] = 0;
        }
    }
    
    for (int i = 1; i <= m; i++) {
        for (int j = 1; j <= n; j++) {
            if (s1[i - 1] == s2[j - 1]) {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = (dp[i - 1][j] > dp[i][j - 1]) ? dp[i - 1][j] : dp[i][j - 1];
            }
        }
    }
    
    int result = dp[m][n];
    for (int i = 0; i <= m; i++) {
        free(dp[i]);
    }
    free(dp);
    return result;
}

// 0/1 Knapsack Problem
int knapsack(int *weights, int *values, int n, int capacity) {
    int *dp = (int *)malloc((capacity + 1) * sizeof(int));
    
    for (int i = 0; i <= capacity; i++) {
        dp[i] = 0;
    }
    
    for (int i = 0; i < n; i++) {
        for (int w = capacity; w >= weights[i]; w--) {
            if (dp[w - weights[i]] + values[i] > dp[w]) {
                dp[w] = dp[w - weights[i]] + values[i];
            }
        }
    }
    
    int result = dp[capacity];
    free(dp);
    return result;
}

// Fibonacci using Dynamic Programming
int fibonacci_dp(int n) {
    if (n <= 1) return n;
    
    int *dp = (int *)malloc((n + 1) * sizeof(int));
    dp[0] = 0;
    dp[1] = 1;
    
    for (int i = 2; i <= n; i++) {
        dp[i] = dp[i - 1] + dp[i - 2];
    }
    
    int result = dp[n];
    free(dp);
    return result;
}

// ============================================================================
// String Processing
// ============================================================================

// KMP (Knuth-Morris-Pratt) pattern matching
void compute_lps(const char *pattern, int *lps, int m) {
    int len = 0;
    lps[0] = 0;
    int i = 1;
    
    while (i < m) {
        if (pattern[i] == pattern[len]) {
            len++;
            lps[i] = len;
            i++;
        } else {
            if (len != 0) {
                len = lps[len - 1];
            } else {
                lps[i] = 0;
                i++;
            }
        }
    }
}

int kmp_search(const char *text, const char *pattern) {
    int n = strlen(text);
    int m = strlen(pattern);
    
    if (m == 0) return 0;
    
    int *lps = (int *)malloc(m * sizeof(int));
    compute_lps(pattern, lps, m);
    
    int i = 0, j = 0;
    int result = -1;
    
    while (i < n) {
        if (pattern[j] == text[i]) {
            i++;
            j++;
        }
        
        if (j == m) {
            result = i - j;
            j = lps[j - 1];
        } else if (i < n && pattern[j] != text[i]) {
            if (j != 0) {
                j = lps[j - 1];
            } else {
                i++;
            }
        }
    }
    
    free(lps);
    return result;
}

// Rabin-Karp rolling hash
#define PRIME 101
#define BASE 256

int rabin_karp_search(const char *text, const char *pattern) {
    int n = strlen(text);
    int m = strlen(pattern);
    if (m == 0 || m > n) return -1;
    
    int i, j;
    int p = 0, t = 0, h = 1;
    
    for (i = 0; i < m - 1; i++)
        h = (h * BASE) % PRIME;
    
    for (i = 0; i < m; i++) {
        p = (BASE * p + pattern[i]) % PRIME;
        t = (BASE * t + text[i]) % PRIME;
    }
    
    for (i = 0; i <= n - m; i++) {
        if (p == t) {
            for (j = 0; j < m; j++) {
                if (text[i + j] != pattern[j])
                    break;
            }
            if (j == m)
                return i;
        }
        
        if (i < n - m) {
            t = (BASE * (t - text[i] * h) + text[i + m]) % PRIME;
            if (t < 0)
                t = (t + PRIME);
        }
    }
    return -1;
}

// ============================================================================
// Sorting Algorithms (Enhanced)
// ============================================================================

void merge(int *arr, int left, int mid, int right) {
    int i = left, j = mid + 1, k = left;
    int *temp = (int *)malloc((right - left + 1) * sizeof(int));
    int temp_idx = 0;
    
    while (i <= mid && j <= right) {
        if (arr[i] <= arr[j]) {
            temp[temp_idx++] = arr[i++];
        } else {
            temp[temp_idx++] = arr[j++];
        }
    }
    
    while (i <= mid) {
        temp[temp_idx++] = arr[i++];
    }
    while (j <= right) {
        temp[temp_idx++] = arr[j++];
    }
    
    for (i = left, temp_idx = 0; i <= right; i++, temp_idx++) {
        arr[i] = temp[temp_idx];
    }
    free(temp);
}

void merge_sort(int *arr, int left, int right) {
    if (left < right) {
        int mid = left + (right - left) / 2;
        merge_sort(arr, left, mid);
        merge_sort(arr, mid + 1, right);
        merge(arr, left, mid, right);
    }
}

int partition(int *arr, int low, int high) {
    int pivot = arr[high];
    int i = low - 1;
    
    for (int j = low; j < high; j++) {
        if (arr[j] < pivot) {
            i++;
            int temp = arr[i];
            arr[i] = arr[j];
            arr[j] = temp;
        }
    }
    
    int temp = arr[i + 1];
    arr[i + 1] = arr[high];
    arr[high] = temp;
    return i + 1;
}

void quick_sort(int *arr, int low, int high) {
    if (low < high) {
        int pi = partition(arr, low, high);
        quick_sort(arr, low, pi - 1);
        quick_sort(arr, pi + 1, high);
    }
}

// Heap sort
void heapify(int *arr, int n, int i) {
    int largest = i;
    int left = 2 * i + 1;
    int right = 2 * i + 2;
    
    if (left < n && arr[left] > arr[largest])
        largest = left;
    
    if (right < n && arr[right] > arr[largest])
        largest = right;
    
    if (largest != i) {
        int temp = arr[i];
        arr[i] = arr[largest];
        arr[largest] = temp;
        heapify(arr, n, largest);
    }
}

void heap_sort(int *arr, int n) {
    for (int i = n / 2 - 1; i >= 0; i--)
        heapify(arr, n, i);
    
    for (int i = n - 1; i > 0; i--) {
        int temp = arr[0];
        arr[0] = arr[i];
        arr[i] = temp;
        heapify(arr, i, 0);
    }
}

// ============================================================================
// Encryption (Simple XOR-based)
// ============================================================================

typedef struct {
    uint32_t state[4];
    int round;
} SimpleStream;

void init_stream(SimpleStream *s, const uint8_t *key, int keylen) {
    s->state[0] = key[0] | (key[1] << 8) | (key[2] << 16) | (key[3] << 24);
    s->state[1] = key[4] | (key[5] << 8) | (key[6] << 16) | (key[7] << 24);
    s->state[2] = 0x12345678;
    s->state[3] = 0x87654321;
    s->round = 0;
}

uint32_t next_state(SimpleStream *s) {
    s->state[0] ^= s->state[1];
    s->state[1] = (s->state[1] << 7) | (s->state[1] >> 25);
    s->state[1] ^= s->state[2];
    s->state[2] = (s->state[2] << 13) | (s->state[2] >> 19);
    s->state[2] ^= s->state[3];
    s->state[3] = (s->state[3] << 3) | (s->state[3] >> 29);
    s->state[3] ^= s->state[0];
    s->round++;
    
    return s->state[0] ^ s->state[3];
}

void encrypt_buffer(uint8_t *data, int len, const uint8_t *key, int keylen) {
    SimpleStream stream;
    init_stream(&stream, key, keylen);
    
    for (int i = 0; i < len; i++) {
        data[i] ^= (uint8_t)next_state(&stream);
    }
}

// ============================================================================
// Main Function
// ============================================================================

int main() {
    printf("Enhanced Medium C Binary - Algorithm Library\n");
    printf("=============================================\n\n");
    
    // Test Dijkstra
    Graph g;
    g.nodes = 5;
    g.edges = 0;
    memset(g.adj_matrix, 0, sizeof(g.adj_matrix));
    
    int dist[MAX_NODES];
    dijkstra(&g, 0, dist);
    printf("Dijkstra algorithm compiled\n");
    
    // Test BFS
    int visited[MAX_NODES] = {0};
    int path[MAX_NODES];
    int path_len = 0;
    bfs(&g, 0, visited, path, &path_len);
    printf("BFS algorithm compiled\n");
    
    // Test Topological Sort
    int topo_result[MAX_NODES];
    topological_sort(&g, topo_result);
    printf("Topological sort compiled\n");
    
    // Test Floyd-Warshall
    int all_pairs[MAX_NODES][MAX_NODES];
    floyd_warshall(&g, all_pairs);
    printf("Floyd-Warshall algorithm compiled\n");
    
    // Test Dynamic Programming
    int lcs_len = lcs_length("ABCDA", "BDCA");
    printf("LCS result: %d\n", lcs_len);
    
    int weights[] = {2, 3, 4, 5};
    int values[] = {3, 4, 5, 6};
    int knapsack_result = knapsack(weights, values, 4, 5);
    printf("Knapsack result: %d\n", knapsack_result);
    
    int fib_result = fibonacci_dp(10);
    printf("Fibonacci(10): %d\n", fib_result);
    
    // Test String processing
    int kmp_pos = kmp_search("hello world", "world");
    int rk_pos = rabin_karp_search("hello world", "world");
    printf("KMP found at: %d, RK found at: %d\n", kmp_pos, rk_pos);
    
    // Test sorting
    int data[] = {64, 34, 25, 12, 22, 11, 90, 88};
    merge_sort(data, 0, 7);
    quick_sort(data, 0, 7);
    heap_sort(data, 8);
    printf("Sorting algorithms compiled\n");
    
    // Test encryption
    uint8_t key[] = "12345678";
    uint8_t buffer[] = "Test encryption data";
    encrypt_buffer(buffer, strlen((char *)buffer), key, 8);
    printf("Encryption compiled\n");
    
    printf("\nEnhanced C compilation successful!\n");
    
    return 0;
}
