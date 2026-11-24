// OLLVM-style obfuscation passes for the WBCQ P3 evaluation corpus
// (RFC #1093). Evaluation tooling only: nothing in Fission links this.
//
//   wbcq-bcf  Bogus control flow. Before a block's body, a branch on the
//             opaque predicate `(x * (x + 1)) & 1 == 0` over an integer
//             argument -- always true -- with a junk block on the false
//             side that does dead arithmetic and rejoins the body.
//   wbcq-fla  Control-flow flattening. Every block but the entry becomes a
//             case of one dispatcher switch on a stack state variable; each
//             block ends by storing its successor's case number. Case
//             numbers are pseudo-random 32-bit values from a seed and the
//             function name, so a build is reproducible.
//
// Run `lower-switch` before `wbcq-fla`, as OLLVM does. Build with
// `build_corpus.py`, which also says how to load the plugin into `opt`.

#include "llvm/IR/Constants.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/PassManager.h"
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Plugins/PassPlugin.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Transforms/Utils/BasicBlockUtils.h"
#include "llvm/Transforms/Utils/Local.h"

#include <cstdint>
#include <set>
#include <vector>

using namespace llvm;

static cl::opt<uint64_t> WbcqSeed("wbcq-seed", cl::init(0x5eed),
                                  cl::desc("Seed for case numbers and block choice"));
static cl::opt<unsigned> WbcqBcfPercent("wbcq-bcf-percent", cl::init(50),
                                        cl::desc("Percent of blocks given a bogus branch"));

namespace {

// SplitMix64: small, and the same on every host.
struct Rng {
  uint64_t State;
  Rng(uint64_t Seed, StringRef Name) : State(Seed) {
    for (char C : Name)
      State = (State ^ static_cast<unsigned char>(C)) * 0x100000001b3ULL;
  }
  uint64_t next() {
    uint64_t Z = (State += 0x9e3779b97f4a7c15ULL);
    Z = (Z ^ (Z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    Z = (Z ^ (Z >> 27)) * 0x94d049bb133111ebULL;
    return Z ^ (Z >> 31);
  }
};

bool unsupported(Function &F) {
  for (BasicBlock &BB : F) {
    Instruction *T = BB.getTerminator();
    if (!isa<BranchInst>(T) && !isa<ReturnInst>(T) && !isa<UnreachableInst>(T))
      return true;
    if (BB.isEHPad() || BB.isLandingPad())
      return true;
  }
  return false;
}

// Demote every PHI and every value used outside its block to the stack, so
// the dispatcher's blocks no longer have to dominate their uses.
void demoteToStack(Function &F) {
  BasicBlock &Entry = F.getEntryBlock();
  for (;;) {
    std::vector<PHINode *> Phis;
    std::vector<Instruction *> Escaping;
    for (BasicBlock &BB : F)
      for (Instruction &I : BB) {
        if (auto *Phi = dyn_cast<PHINode>(&I)) {
          Phis.push_back(Phi);
          continue;
        }
        if (isa<AllocaInst>(I) && I.getParent() == &Entry)
          continue;
        if (I.isUsedOutsideOfBlock(&BB))
          Escaping.push_back(&I);
      }
    if (Phis.empty() && Escaping.empty())
      return;
    // Stack slots go at the very top of the entry, ahead of every value
    // they will hold.
    for (Instruction *I : Escaping)
      DemoteRegToStack(*I, false, Entry.getFirstInsertionPt());
    for (PHINode *Phi : Phis)
      DemotePHIToStack(Phi, Entry.getFirstInsertionPt());
  }
}

struct FlattenPass : PassInfoMixin<FlattenPass> {
  PreservedAnalyses run(Function &F, FunctionAnalysisManager &) {
    if (F.isDeclaration() || F.size() <= 1 || unsupported(F))
      return PreservedAnalyses::all();
    LLVMContext &Ctx = F.getContext();
    Rng R(WbcqSeed, F.getName());
    BasicBlock *Entry = &F.getEntryBlock();

    // The entry keeps its straight-line code and hands over to the
    // dispatcher; its own branch becomes the first flattened block.
    Instruction *EntryTerm = Entry->getTerminator();
    if (!isa<BranchInst>(EntryTerm) || cast<BranchInst>(EntryTerm)->isConditional())
      SplitBlock(Entry, EntryTerm);
    BasicBlock *First = Entry->getTerminator()->getSuccessor(0);

    std::vector<BasicBlock *> Blocks;
    for (BasicBlock &BB : F)
      if (&BB != Entry)
        Blocks.push_back(&BB);

    std::set<uint32_t> Used;
    DenseMap<BasicBlock *, ConstantInt *> Case;
    IntegerType *I32 = Type::getInt32Ty(Ctx);
    for (BasicBlock *BB : Blocks) {
      uint32_t V;
      do
        V = static_cast<uint32_t>(R.next());
      while (!Used.insert(V).second);
      Case[BB] = ConstantInt::get(I32, V);
    }

    IRBuilder<> EntryB(Entry->getTerminator());
    AllocaInst *State = EntryB.CreateAlloca(I32, nullptr, "wbcq.state");
    EntryB.CreateStore(Case[First], State);

    BasicBlock *Dispatch = BasicBlock::Create(Ctx, "wbcq.dispatch", &F, First);
    BasicBlock *Default = BasicBlock::Create(Ctx, "wbcq.default", &F);
    new UnreachableInst(Ctx, Default);
    IRBuilder<> DispatchB(Dispatch);
    SwitchInst *Switch = DispatchB.CreateSwitch(
        DispatchB.CreateLoad(I32, State, "wbcq.next"), Default, Blocks.size());
    for (BasicBlock *BB : Blocks)
      Switch->addCase(Case[BB], BB);

    Entry->getTerminator()->eraseFromParent();
    BranchInst::Create(Dispatch, Entry);

    for (BasicBlock *BB : Blocks) {
      auto *Br = dyn_cast<BranchInst>(BB->getTerminator());
      if (!Br)
        continue; // return, unreachable
      IRBuilder<> B(Br);
      Value *Next;
      if (Br->isUnconditional())
        Next = Case[Br->getSuccessor(0)];
      else
        Next = B.CreateSelect(Br->getCondition(), Case[Br->getSuccessor(0)],
                              Case[Br->getSuccessor(1)], "wbcq.select");
      B.CreateStore(Next, State);
      B.CreateBr(Dispatch);
      Br->eraseFromParent();
    }
    demoteToStack(F);
    return PreservedAnalyses::none();
  }
};

struct BogusFlowPass : PassInfoMixin<BogusFlowPass> {
  PreservedAnalyses run(Function &F, FunctionAnalysisManager &) {
    if (F.isDeclaration())
      return PreservedAnalyses::all();
    Argument *X = nullptr;
    for (Argument &A : F.args())
      if (A.getType()->isIntegerTy(32)) {
        X = &A;
        break;
      }
    if (!X)
      return PreservedAnalyses::all();
    Rng R(WbcqSeed ^ 0xbcf, F.getName());
    std::vector<BasicBlock *> Chosen;
    for (BasicBlock &BB : F)
      if (&BB != &F.getEntryBlock() && R.next() % 100 < WbcqBcfPercent)
        Chosen.push_back(&BB);
    if (Chosen.empty())
      return PreservedAnalyses::all();
    LLVMContext &Ctx = F.getContext();
    for (BasicBlock *BB : Chosen) {
      BasicBlock *Body = SplitBlock(BB, BB->getFirstNonPHIOrDbgOrLifetime());
      Body->setName(BB->getName() + ".body");
      BasicBlock *Junk =
          BasicBlock::Create(Ctx, BB->getName() + ".junk", &F, Body);
      IRBuilder<> J(Junk);
      // Dead arithmetic shaped like real work; it never runs.
      Value *T = J.CreateMul(X, ConstantInt::get(X->getType(), 0x2f5b));
      T = J.CreateXor(T, ConstantInt::get(X->getType(), 0x1337));
      T = J.CreateAdd(T, X);
      (void)T;
      J.CreateBr(Body);
      BB->getTerminator()->eraseFromParent();
      IRBuilder<> H(BB);
      Value *XPlus = H.CreateAdd(X, ConstantInt::get(X->getType(), 1));
      Value *Prod = H.CreateMul(X, XPlus);
      Value *Odd = H.CreateAnd(Prod, ConstantInt::get(X->getType(), 1));
      Value *Even = H.CreateICmpEQ(Odd, ConstantInt::get(X->getType(), 0), "wbcq.opaque");
      H.CreateCondBr(Even, Body, Junk);
    }
    return PreservedAnalyses::none();
  }
};

} // namespace

extern "C" LLVM_ATTRIBUTE_WEAK PassPluginLibraryInfo llvmGetPassPluginInfo() {
  return {LLVM_PLUGIN_API_VERSION, "WbcqObfuscator", "0.1", [](PassBuilder &PB) {
            PB.registerPipelineParsingCallback(
                [](StringRef Name, FunctionPassManager &FPM,
                   ArrayRef<PassBuilder::PipelineElement>) {
                  if (Name == "wbcq-fla") {
                    FPM.addPass(FlattenPass());
                    return true;
                  }
                  if (Name == "wbcq-bcf") {
                    FPM.addPass(BogusFlowPass());
                    return true;
                  }
                  return false;
                });
          }};
}
