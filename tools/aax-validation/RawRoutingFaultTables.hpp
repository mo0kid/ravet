#pragma once
// Authored coherent native consumer facade: actual canonical Rust handles and
// metadata, with explicit public origin callbacks and controlled process faults.
// No copied-core admission: every companion points at this selected facade.
namespace raw_fault {
inline RavetNativeProcessorEntryV1 core{};
inline RavetNativeParameterEntryV1 parameters{};
inline RavetNativeCachedParameterEntryV1 cache{};
inline RavetNativeAudioStateEntryV1 audio{};
inline RavetNativeStateEntryV1 state{};
inline RavetNativeTelemetryEntryV1 telemetry{};
inline const RavetNativeProcessorEntryV1* actualCore=nullptr;
inline const RavetNativeCachedParameterEntryV1* actualCache=nullptr;
inline const RavetNativeTelemetryEntryV1* actualTelemetry=nullptr;
inline const RavetNativeStateEntryV1* actualState=nullptr;
inline uint32_t created=0,destroyed=0,cacheCreated=0,cacheDestroyed=0,telemetryCreated=0,telemetryDestroyed=0,processCalls=0,stateCreated=0,stateDestroyed=0;
inline void (*processHook)()=nullptr;
inline std::atomic<bool> hookEntered{false},hookRelease{false};
inline int32_t processStatus=0,prepareStatus=0;inline bool processThrow=false,telemetryQueryThrow=false,telemetryCreateRefuse=false;
inline const RavetNativeProcessorEntryV1* coreOrigin(){return &core;}
inline const RavetNativeProcessorEntryV1* telemetryOrigin(){if(telemetryQueryThrow)throw 39;return &core;}
inline const RavetNativeParameterEntryV1* parametersOrigin(){return &parameters;}
inline const RavetNativeCachedParameterEntryV1* cacheOrigin(){return &cache;}
inline void* create(){++created;return actualCore->create();}
inline void destroy(void* p){++destroyed;actualCore->destroy(p);}
inline void* createCache(){++cacheCreated;return actualCache->create();}
inline void destroyCache(void* p){++cacheDestroyed;actualCache->destroy(p);}
inline void* createTelemetry(){if(telemetryCreateRefuse)return nullptr;++telemetryCreated;return actualTelemetry->create_cache();}
inline void destroyTelemetry(void* p){++telemetryDestroyed;actualTelemetry->destroy_cache(p);}
inline void* createState(const uint8_t* bytes,uint32_t length){auto* out=actualState->create(bytes,length);if(out)++stateCreated;return out;}
inline void destroyState(void* p){++stateDestroyed;actualState->destroy(p);}
inline int32_t prepare(void* p,const RavetNativeProcessorConfigV1* config){return prepareStatus?prepareStatus:actualCore->prepare(p,config);}
inline int32_t process(void* p,const RavetNativeProcessorBusV1* buses,uint32_t count,uint32_t frames,const RavetNativeEventV1* events,uint32_t eventCount,const RavetNativeTransportV1* transport,RavetNativeEventV1* out,uint32_t capacity,RavetNativeProcessorResultV1* report){++processCalls;if(processHook)processHook();if(processThrow)throw 97;if(processStatus)return processStatus;return actualCore->process(p,buses,count,frames,events,eventCount,transport,out,capacity,report);}
inline void initialize(bool delay){
 const auto bits=RawRoutingBits;
 actualCore=delay?ravet_raw_delay16_processor_v1(bits):ravet_raw_bus_routing_processor_v1(bits);
 actualCache=delay?ravet_raw_delay16_cached_parameters_v1(bits):ravet_raw_bus_routing_cached_parameters_v1(bits);
 actualTelemetry=delay?ravet_raw_delay16_telemetry_v1(bits):ravet_raw_bus_routing_telemetry_v1(bits);
 core=*actualCore;core.create=create;core.destroy=destroy;core.process=process;core.prepare=prepare;
 parameters=*(delay?ravet_raw_delay16_parameters_v1(bits):ravet_raw_bus_routing_parameters_v1(bits));parameters.processor_entry=coreOrigin;
 cache=*actualCache;cache.parameters_entry=parametersOrigin;cache.create=createCache;cache.destroy=destroyCache;
 audio=*(delay?ravet_raw_delay16_audio_state_v1(bits):ravet_raw_bus_routing_audio_state_v1(bits));audio.cached_parameters_entry=cacheOrigin;
 actualState=delay?ravet_raw_delay16_state_v1(bits):ravet_raw_bus_routing_state_v1(bits);state=*actualState;state.parameters_entry=parametersOrigin;state.create=createState;state.destroy=destroyState;
 telemetry=*actualTelemetry;telemetry.processor_entry=telemetryOrigin;telemetry.create_cache=createTelemetry;telemetry.destroy_cache=destroyTelemetry;
 created=destroyed=cacheCreated=cacheDestroyed=telemetryCreated=telemetryDestroyed=processCalls=stateCreated=stateDestroyed=0;processStatus=prepareStatus=0;processThrow=telemetryQueryThrow=telemetryCreateRefuse=false;processHook=nullptr;hookEntered=false;hookRelease=false;
}
inline AAX_IEffectParameters* AAX_CALLBACK factory(){RavetRawMainModel::Identity id{RawRoutingIDs[0],1,0x52617674,0x52765277,0x52765231,0x52765253};return new RavetRawMainModel(id,&core,&audio,&cache,&state,&telemetry);}
}
