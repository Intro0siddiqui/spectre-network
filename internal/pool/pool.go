package pool

import (
	"sync"
)

// Task is a function that returns an error.
type Task func() error

// Pool manages a set of workers to execute tasks concurrently.
type Pool struct {
	workers int
	tasks   chan Task
	errs    chan error
	wg      sync.WaitGroup
	once    sync.Once
	done    chan struct{}
}

// NewPool creates a new worker pool with the specified number of workers.
func NewPool(workers int) *Pool {
	p := &Pool{
		workers: workers,
		tasks:   make(chan Task),
		errs:    make(chan error, workers),
		done:    make(chan struct{}),
	}
	
	for i := 0; i < workers; i++ {
		p.wg.Add(1)
		go p.worker()
	}
	
	return p
}

func (p *Pool) worker() {
	defer p.wg.Done()
	for {
		select {
		case task, ok := <-p.tasks:
			if !ok {
				return
			}
			if err := task(); err != nil {
				select {
				case p.errs <- err:
				default:
				}
			}
		case <-p.done:
			return
		}
	}
}

// Submit adds a task to the pool.
func (p *Pool) Submit(t Task) {
	select {
	case p.tasks <- t:
	case <-p.done:
	}
}

// Wait closes the task channel and waits for all workers to finish.
// It returns the first error encountered, if any.
func (p *Pool) Wait() error {
	p.once.Do(func() {
		close(p.tasks)
	})
	p.wg.Wait()
	
	select {
	case err := <-p.errs:
		return err
	default:
		return nil
	}
}

// Stop closes the pool immediately.
func (p *Pool) Stop() {
	close(p.done)
}
