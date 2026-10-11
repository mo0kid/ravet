#include "instance-fixture-objects.hpp"
int main(){try{
 Observation state;state.fail="none";Factory factory(state);auto host=owned(new HostApplication);
 ControllerHandlerKeepers handlers;auto instance=Instance::create(factory,*host,metadata(),0,std::make_shared<int>(1),handlers);instance->prepare({48000,32,2,2});instance->start();instance->beginAudio();
 std::array<std::array<float,40>,2> input{},output{};for(auto& plane:input)plane.fill(.25f);for(auto& plane:output)plane.fill(77.f);
 std::array<RavetVst3InputChannel,2> in{{{input[0].data(),40,0},{input[1].data(),40,0}}};
 std::array<RavetVst3OutputChannel,2> out{{{output[0].data(),40,0},{output[1].data(),40,0}}};
 RavetVst3AudioBlock block{sizeof(block),32,2,2,in.data(),out.data()};
 std::thread audio([&]{
  in[1].capacity=31;require(instance->processBlock(block)==kInvalidArgument&&state.processes==0&&output[0][0]==77.f);in[1].capacity=40;
  out[1].reserved=1;require(instance->processBlock(block)==kInvalidArgument&&state.processes==0&&output[0][0]==77.f);out[1].reserved=0;
  auto* previous=out[1].samples;out[1].samples=reinterpret_cast<float*>(reinterpret_cast<char*>(previous)+1);require(instance->processBlock(block)==kInvalidArgument&&state.processes==0);out[1].samples=previous;
  require(instance->processBlock(block)==kResultOk&&state.processes==1);for(auto& p:output){for(size_t f=0;f<32;++f)require(p[f]==.75f);for(size_t f=32;f<40;++f)require(p[f]==77.f);}
  state.fail="process";require(instance->processBlock(block)==rejected&&state.processes==2);for(auto& p:output){for(size_t f=0;f<32;++f)require(p[f]==0.f);for(size_t f=32;f<40;++f)require(p[f]==77.f);}
  for(auto& p:output)p.fill(88.f);require(instance->processBlock(block)==kInvalidArgument&&state.processes==2&&output[0][0]==88.f);
 });audio.join();instance->endAudio();require(instance->stop().native==kResultOk);require(instance->retire().native==kResultOk);instance.reset();require(state.destroyed==1&&state.initialized==state.terminated);
 std::cout<<"PASS caller block late input/output/alignment rejection preserves native history, returned foreign process fault silences prefixes/tails and fail-closed repeated block never enters DSP\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
