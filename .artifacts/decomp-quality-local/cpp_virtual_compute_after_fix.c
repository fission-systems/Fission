// ============================================
// Function: _cpp_virtual_compute @ 0x100000730
// ============================================


int _cpp_virtual_compute(int x)

{
  int iVar1;
  longlong *plVar2;
  
  plVar2 = (longlong *)__Znwm(8);
  *plVar2 = 0;
  sub_1000007a8();
  iVar1 = /* virtual method @0x10 */ (**(code **)(*plVar2 + 0x10))(plVar2,x);
  if (plVar2 != (longlong *)0x0) {
    /* virtual dtor @8 */ (**(code **)(*plVar2 + 8))(plVar2);
  }
  return iVar1;
}


