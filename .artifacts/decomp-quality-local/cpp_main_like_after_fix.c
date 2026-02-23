// ============================================
// Function: _cpp_main_like @ 0x1000007d4
// ============================================


/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

int _cpp_main_like(void)

{
  int iVar1;
  int iVar2;
  Item *item;
  int local_30 [6];
  longlong local_18;
  
  local_18 = *____stack_chk_guard;
  _puts("=== C++ decompiler benchmark ===");
  local_30[2] = 3;
  local_30[3] = 4;
  local_30[0] = 1;
  local_30[1] = 2;
  local_30[4] = 5;
  iVar1 = _cpp_sum_array((int *)local_30,5);
  item = _cpp_create_item(0x7ea,"CppItem",12.5);
  iVar2 = _cpp_switch(2);
  iVar1 = _cpp_add(iVar1,iVar2);
  iVar2 = _cpp_virtual_compute(7);
  iVar1 = _cpp_add(iVar1,iVar2);
  if (item != (Item *)0x0) {
    _printf("Item %d %s %.2f\n",item->id,item->name,item->value);
    _cpp_destroy_item(item);
  }
  if (*____stack_chk_guard - local_18 != 0) {
    ___stack_chk_fail(*____stack_chk_guard - local_18);
  }
  return iVar1;
}


