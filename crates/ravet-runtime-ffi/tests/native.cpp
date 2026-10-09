#include "RavetRuntime.hpp"
#include <atomic>
#include <chrono>
#include <future>
#include <iostream>
#include <thread>
static void require(bool ok,const char* text) { if(!ok) throw std::runtime_error(text); }
int main() {
    try {
        using namespace ravet::runtime;
        auto root=std::filesystem::temp_directory_path()/"ravet-native-abi-test";
        auto file=root/utf8Path("preset-Δ.bin");
        require(atomicWrite(file,reinterpret_cast<const uint8_t*>("a\0b"),3),"write");
        require(readFile(file,3)==std::string("a\0b",3),"bounded binary read");
        require(readFile(file,2).empty(),"lossy facade limit");
        require(atomicWrite(file,nullptr,0)&&readFile(file,0).empty(),"empty write");
        require(!modulePath(reinterpret_cast<const void*>(&main)).empty(),"module query");
        std::atomic<int> completed{0};
        {
            std::promise<void> started,release;
            auto wait=release.get_future().share();
            SerialExecutor queue(1); // Drains before the referenced promise is destroyed.
            require(queue.submit([&,wait] { started.set_value();wait.wait(); }),"gate submission");
            started.get_future().wait();
            std::string text(4096,'x');
            require(queue.submit([copy=text,&completed] { require(copy==std::string(4096,'x'),"owning capture");++completed; }),"value task");
            text.clear();
            require(!queue.submit([&] { completed=99; }),"bounded queue");
            release.set_value();
        }
        require(completed==1,"drained/rejected task behavior");
        auto queue=std::make_shared<SerialExecutor>();
        std::promise<void> release,selfDone;
        auto wait=release.get_future().share();
        require(queue->submit([owner=queue,wait,&selfDone]() mutable { wait.wait();owner.reset();selfDone.set_value(); }),"self ownership");
        queue.reset();release.set_value();
        require(selfDone.get_future().wait_for(std::chrono::seconds(2))==std::future_status::ready,"self teardown");
        std::atomic<int> ticks{0};
        std::promise<void> ticked;
        PeriodicTask timer;
        timer.start(5,[&] { if(++ticks==1) ticked.set_value(); });
        require(ticked.get_future().wait_for(std::chrono::seconds(2))==std::future_status::ready,"timer");
        timer.stop();int stopped=ticks;
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
        require(ticks==stopped,"callback after timer shutdown");
        require(postJSON("http://localhost/","{}","RAVET native test").error==1,"invalid transport request");
        std::filesystem::remove_all(root);
        std::cout<<"Native facade: binary files, UTF-8 paths, owned tasks, queue overflow, self teardown and Rust timer passed\n";
        return 0;
    } catch(const std::exception& error) { std::cerr<<error.what()<<'\n';return 1; }
}
