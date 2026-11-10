namespace geo {
struct Shape { long id; double scale; Shape(long i) : id(i), scale(1.0) {} double area(int mode) const; };
struct Named { int len; char tag[12]; int checksum(int seed) const; };
struct Circle : Shape, Named { double r; double perimeter(double k) const; Circle(long i, double rr) : Shape(i), r(rr) { len = 3; } };
double Shape::area(int mode) const { return scale * mode + id; }
int Named::checksum(int seed) const { int s = seed; for (int i = 0; i < len; i++) s = s * 31 + tag[i]; return s; }
double Circle::perimeter(double k) const { return 2.0 * r * k + area(1) + checksum(7); }
}
template <typename T> struct Box { T v; T twice() const; };
template <typename T> T Box<T>::twice() const { return v + v; }
template struct Box<int>;
template struct Box<long>;
int run(const geo::Circle *c, const Box<int> *b, const Box<long> *l) { return (int)c->perimeter(2.5) + b->twice() + (int)l->twice(); }
int main(int argc, char **argv) { geo::Circle c(argc, 1.5); Box<int> b{argc}; Box<long> l{argc}; return run(&c, &b, &l); }
