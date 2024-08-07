import ctypes
from ctypes import wintypes
class Singleton:
    # Constants

    # Create a mutex
    def create_mutex(mutex_name):
        ERROR_ALREADY_EXISTS = 183
        # CreateMutex function from kernel32.dll
        kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        
        # Create the mutex
        mutex_handle = kernel32.CreateMutexW(None, False, mutex_name)
        
        # Check for errors
        if mutex_handle is None:
            raise ctypes.WinError(ctypes.get_last_error())
        
        # Check if the mutex already exists
        if ctypes.GetLastError() == ERROR_ALREADY_EXISTS:
            print(f"Mutex '{mutex_name}' already exists.")
            return False
        
        print(f"Mutex '{mutex_name}' created successfully.")
        return True

    # Release the mutex
    def release_mutex(mutex_name):
        kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)

        # Open the mutex
        mutex_handle = kernel32.OpenMutexW(0x1F0001, False, mutex_name)  # MUTEX_ALL_ACCESS
        if mutex_handle is None:
            raise ctypes.WinError(ctypes.get_last_error())

        # Release the mutex
        result = kernel32.ReleaseMutex(mutex_handle)
        if not result:
            raise ctypes.WinError(ctypes.get_last_error())

        # Close the mutex handle
        kernel32.CloseHandle(mutex_handle)
        print(f"Mutex '{mutex_name}' released successfully.")
