/* ###
 * IP: GHIDRA
 * NOTE: Stub header for systems without GNU BFD library
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 * 
 *      http://www.apache.org/licenses/LICENSE-2.0
 * 
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
// Stub for systems without GNU BFD library

#ifndef __LOADIMAGE_BFD_HH__
#define __LOADIMAGE_BFD_HH__

#include "loadimage.hh"

namespace ghidra {

struct ImportRecord {
  string dllname;
  string funcname;
  int ordinal;
  Address address;
  Address thunkaddress;
};

// Stub class when BFD is not available
// Fission uses Rust's goblin crate for binary loading instead
class LoadImageBfd : public LoadImage {
public:
  LoadImageBfd(const string &f, const string &t) : LoadImage(f) { (void)t; }
  void attachToSpace(AddrSpace *id) { (void)id; }
  void open(void) { throw LowlevelError("BFD not available - use Rust goblin instead"); }
  void close(void) {}
  void getImportTable(vector<ImportRecord> &irec) { throw LowlevelError("Not implemented"); }
  virtual ~LoadImageBfd(void) {}
  virtual void loadFill(uint1 *ptr, int4 size, const Address &addr) { 
    (void)ptr; (void)size; (void)addr;
    throw LowlevelError("BFD not available"); 
  }
  virtual void openSymbols(void) const {}
  virtual void closeSymbols(void) const {}
  virtual bool getNextSymbol(LoadImageFunc &record) const { (void)record; return false; }
  virtual void openSectionInfo(void) const {}
  virtual void closeSectionInfo(void) const {}
  virtual bool getNextSection(LoadImageSection &sec) const { (void)sec; return false; }
  virtual void getReadonly(RangeList &list) const { (void)list; }
  virtual string getArchType(void) const { return "stub"; }
  virtual void adjustVma(long adjust) { (void)adjust; }
};

} // End namespace ghidra
#endif
