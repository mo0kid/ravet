#pragma once
// Shared validation and lookup for ravet_web_create_assets (all backends).
#include "RavetWebView.h"
#include <cstring>
#include <map>
#include <string>

namespace ravet_web_assets {
constexpr size_t maximumCount=4096, maximumPath=512;
constexpr size_t maximumAsset=size_t(64)<<20, maximumTotal=size_t(256)<<20;
struct Asset { std::string mime; const void* data; size_t size; };
using Table=std::map<std::string,Asset>;
inline bool validPath(const char* path) {
    if(!path) return false;
    size_t n=strnlen(path,maximumPath+1);
    if(!n||n>maximumPath||path[0]=='/'||path[n-1]=='/') return false;
    for(size_t i=0;i<n;++i) {
        char c=path[i];
        bool ok=(c>='a'&&c<='z')||(c>='A'&&c<='Z')||(c>='0'&&c<='9')||c=='.'||c=='_'||c=='-'||c=='/';
        if(!ok||(c=='/'&&path[i+1]=='/')) return false;
    }
    // No "." or ".." segments.
    std::string text(path,n); size_t start=0;
    while(start<=n) {
        size_t end=text.find('/',start); if(end==std::string::npos) end=n;
        auto segment=text.substr(start,end-start);
        if(segment=="."||segment=="..") return false;
        start=end+1;
    }
    return text!="index.html";
}
inline bool validMime(const char* mime) {
    if(!mime) return false; size_t n=strnlen(mime,128); if(!n||n>127) return false;
    for(size_t i=0;i<n;++i) if(mime[i]<0x20||mime[i]>0x7e) return false;
    return true;
}
// Builds the table, or returns false for any invalid entry.
inline bool build(const RavetWebAsset* assets,size_t count,Table& table) {
    if(count>maximumCount||(count&&!assets)) return false;
    size_t total=0;
    for(size_t i=0;i<count;++i) {
        const auto& a=assets[i];
        if(!validPath(a.path)||!validMime(a.mime)||(a.size&&!a.data)||a.size>maximumAsset) return false;
        total+=a.size; if(total>maximumTotal) return false;
        if(!table.emplace(a.path,Asset{a.mime,a.data,a.size}).second) return false;
    }
    return true;
}
// Path of a request URI under `origin` (which ends in "/"), without query or
// fragment; empty when the URI is outside the origin.
inline std::string pathOf(const std::string& uri,const std::string& origin) {
    if(uri.compare(0,origin.size(),origin)) return {};
    auto path=uri.substr(origin.size());
    auto cut=path.find_first_of("?#"); if(cut!=std::string::npos) path.resize(cut);
    return path;
}
}
