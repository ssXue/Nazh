/*
 * macOS OSAL — 基于 Linux OSAL 适配，替换 clock_nanosleep 为 nanosleep。
 * 仅用于 Nazh soem-sys 编译。
 */
#include <osal.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <pthread.h>

void osal_get_monotonic_time(ec_timet *ts)
{
   clock_gettime(CLOCK_MONOTONIC, ts);
}

ec_timet osal_current_time(void)
{
   struct timespec ts;
   clock_gettime(CLOCK_REALTIME, &ts);
   return ts;
}

void osal_time_diff(ec_timet *start, ec_timet *end, ec_timet *diff)
{
   osal_timespecsub(end, start, diff);
}

void osal_timer_start(osal_timert *self, uint32 timeout_usec)
{
   struct timespec start_time;
   struct timespec timeout;

   osal_get_monotonic_time(&start_time);
   osal_timespec_from_usec(timeout_usec, &timeout);
   osal_timespecadd(&start_time, &timeout, &self->stop_time);
}

boolean osal_timer_is_expired(osal_timert *self)
{
   struct timespec current_time;
   int is_not_yet_expired;

   osal_get_monotonic_time(&current_time);
   is_not_yet_expired = osal_timespeccmp(&current_time, &self->stop_time, <);

   return is_not_yet_expired == FALSE;
}

int osal_usleep(uint32 usec)
{
   struct timespec ts;
   osal_timespec_from_usec(usec, &ts);
   return nanosleep(&ts, NULL) == 0 ? 0 : -1;
}

int osal_monotonic_sleep(ec_timet *ts)
{
   struct timespec now;
   struct timespec diff;
   osal_get_monotonic_time(&now);
   if (osal_timespeccmp(ts, &now, <=))
   {
      return 0;
   }
   osal_timespecsub(ts, &now, &diff);
   return nanosleep(&diff, NULL) == 0 ? 0 : -1;
}

void *osal_malloc(size_t size)
{
   return malloc(size);
}

void osal_free(void *ptr)
{
   free(ptr);
}

int osal_thread_create(void *thandle, int stacksize, void *func, void *param)
{
   int ret;
   pthread_attr_t attr;
   pthread_t *threadp;

   threadp = thandle;
   pthread_attr_init(&attr);
   pthread_attr_setstacksize(&attr, (size_t)stacksize);
   ret = pthread_create(threadp, &attr, func, param);
   pthread_attr_destroy(&attr);
   if (ret < 0)
   {
      return 0;
   }
   return 1;
}

int osal_thread_create_rt(void *thandle, int stacksize, void *func, void *param)
{
   return osal_thread_create(thandle, stacksize, func, param);
}

void *osal_mutex_create(void)
{
   pthread_mutexattr_t mutexattr;
   osal_mutext *mutex;
   mutex = (osal_mutext *)osal_malloc(sizeof(osal_mutext));
   if (mutex)
   {
      pthread_mutexattr_init(&mutexattr);
      pthread_mutexattr_setprotocol(&mutexattr, PTHREAD_PRIO_INHERIT);
      pthread_mutex_init(mutex, &mutexattr);
      pthread_mutexattr_destroy(&mutexattr);
   }
   return (void *)mutex;
}

void osal_mutex_destroy(void *mutex)
{
   pthread_mutex_destroy((osal_mutext *)mutex);
   osal_free(mutex);
}

void osal_mutex_lock(void *mutex)
{
   pthread_mutex_lock((osal_mutext *)mutex);
}

void osal_mutex_unlock(void *mutex)
{
   pthread_mutex_unlock((osal_mutext *)mutex);
}
