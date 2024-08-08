import ctypes

class Singleton:
    def create_mutex(mutex_name):
        ERROR_ALREADY_EXISTS = 396
        kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        mutex_handle = kernel32.CreateMutexW(None, False, mutex_name)
        if mutex_handle is None:
            raise ctypes.WinError(ctypes.get_last_error())
        if ctypes.get_last_error() == ERROR_ALREADY_EXISTS:
            print(f"Mutex '{mutex_name}' already exists.")
            return False
        print(f"Mutex '{mutex_name}' created successfully.")
        return True
    def release_mutex(mutex_name):
        kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        mutex_handle = kernel32.OpenMutexW(0x1F0001, False, mutex_name)  # MUTEX_ALL_ACCESS
        if mutex_handle is None:
            raise ctypes.WinError(ctypes.get_last_error())
        result = kernel32.ReleaseMutex(mutex_handle)
        if not result:
            raise ctypes.WinError(ctypes.get_last_error())
        kernel32.CloseHandle(mutex_handle)
        print(f"Mutex '{mutex_name}' released successfully.")