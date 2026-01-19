#import <Foundation/Foundation.h>

@interface FissionTester : NSObject
- (void)sayHello;
- (int)addNumber:(int)a toNumber:(int)b;
@end

@implementation FissionTester
- (void)sayHello {
    NSLog(@"Hello from Objective-C!");
}
- (int)addNumber:(int)a toNumber:(int)b {
    return a + b;
}
@end

int main(int argc, const char * argv[]) {
    @autoreleasepool {
        FissionTester *tester = [[FissionTester alloc] init];
        [tester sayHello];
        int res = [tester addNumber:5 toNumber:10];
        NSLog(@"Result: %d", res);
    }
    return 0;
}
