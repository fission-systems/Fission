typedef unsigned int u32;
typedef unsigned char u8;
typedef unsigned short u16;

volatile u32 data_sink = 0;

struct Point {
    u16 x;
    u16 y;
};

struct Rect {
    struct Point top_left;
    struct Point bottom_right;
};

union Value {
    u32 i;
    u8 b[4];
};

struct Config {
    u8 type;
    union Value val;
    struct Rect bounds;
};

u32 process_config(struct Config* cfg) {
    u32 result = 0;
    if (cfg->type == 1) {
        result += cfg->val.i;
    } else {
        result += cfg->val.b[0] + cfg->val.b[1] + cfg->val.b[2] + cfg->val.b[3];
    }
    
    u32 width = cfg->bounds.bottom_right.x - cfg->bounds.top_left.x;
    u32 height = cfg->bounds.bottom_right.y - cfg->bounds.top_left.y;
    
    result += width * height;
    return result;
}

void run_data_structures(u32 seed) {
    struct Config cfg;
    cfg.type = seed % 2;
    cfg.val.i = seed * 17;
    cfg.bounds.top_left.x = seed & 0xFF;
    cfg.bounds.top_left.y = (seed >> 8) & 0xFF;
    cfg.bounds.bottom_right.x = cfg.bounds.top_left.x + 100;
    cfg.bounds.bottom_right.y = cfg.bounds.top_left.y + 50;
    
    struct Config configs[3];
    for(int i = 0; i < 3; i++) {
        configs[i] = cfg;
        configs[i].type = (seed + i) % 2;
    }
    
    u32 total = 0;
    for(int i = 0; i < 3; i++) {
        total += process_config(&configs[i]);
    }
    data_sink = total;
}
